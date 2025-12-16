"""
Test Generator Service for Surveyor Agent.

Generates pytest tests for deployed plugins using LLM-based code generation.
"""

import ast
import logging
from dataclasses import dataclass
from typing import Optional
from pathlib import Path

from casparian_flow.db.models import PluginManifest, FileLocation
from casparian_flow.services.ai_types import SchemaProposal
from casparian_flow.services.llm_generator import LLMGenerator

logger = logging.getLogger(__name__)


@dataclass
class GeneratedTest:
    """Result of test generation."""
    test_file_path: str
    test_code: str
    success: bool
    error_message: Optional[str] = None


class TestGenerator:
    """
    Generates pytest tests for deployed plugins.

    Uses existing conftest.py fixtures and follows project patterns.
    """

    def __init__(self, llm_generator: LLMGenerator):
        self.llm_generator = llm_generator

    def generate_test(
        self,
        plugin_manifest: PluginManifest,
        schema_proposal: SchemaProposal,
        sample_file: FileLocation,
    ) -> GeneratedTest:
        """
        Generate a pytest test file for a plugin.

        Strategy:
        1. Create test data fixture
        2. Test plugin execution
        3. Verify output schema matches proposal
        4. Check error handling

        Args:
            plugin_manifest: Deployed plugin
            schema_proposal: Original schema proposal
            sample_file: Sample input file for test

        Returns:
            GeneratedTest with test code
        """
        logger.info(f"Generating test for plugin {plugin_manifest.plugin_name}")

        # Build LLM prompt
        prompt = self._build_test_prompt(
            plugin_manifest.source_code,
            schema_proposal,
            sample_file.rel_path,
        )

        try:
            # Call LLM to generate test code
            response = self.llm_generator.provider.chat_completion(
                messages=[{"role": "user", "content": prompt}],
                model=self.llm_generator.provider.default_model,
                json_mode=False,
            )

            # Extract code from markdown fence
            test_code = self._extract_code_from_markdown(response)

            # Validate syntax
            if not self.validate_test_syntax(test_code):
                return GeneratedTest(
                    test_file_path="",
                    test_code=test_code,
                    success=False,
                    error_message="Generated test code has invalid Python syntax",
                )

            # Write test file
            test_file_path = self.write_test_file(test_code, plugin_manifest.plugin_name)

            logger.info(f"Test generated successfully: {test_file_path}")

            return GeneratedTest(
                test_file_path=test_file_path,
                test_code=test_code,
                success=True,
                error_message=None,
            )

        except Exception as e:
            logger.exception(f"Test generation failed: {e}")
            return GeneratedTest(
                test_file_path="",
                test_code="",
                success=False,
                error_message=str(e),
            )

    def _build_test_prompt(
        self,
        plugin_code: str,
        schema: SchemaProposal,
        sample_path: str,
    ) -> str:
        """Build LLM prompt for test generation."""
        prompt = f"""Generate a pytest test file for the following plugin code.

PLUGIN CODE:
```python
{plugin_code}
```

EXPECTED SCHEMA:
- File Type: {schema.file_type_inferred}
- Target Topic: {schema.target_topic}
- Columns: {', '.join(schema.columns) if schema.columns else 'Not specified'}
- Read Strategy: {schema.read_strategy}

SAMPLE FILE PATH: {sample_path}

REQUIREMENTS:
1. Use existing pytest fixtures from conftest.py:
   - test_db_engine: Test database engine
   - test_db_session: Test database session
   - test_source_root: Test source root ID
   - temp_test_dir: Temporary test directory

2. Create a test function that:
   - Sets up a sample input file similar to the provided path
   - Instantiates the plugin Handler class
   - Calls configure() and execute() methods
   - Verifies the output matches the expected schema
   - Checks that data was published to the correct topic

3. Follow these patterns:
   - Use descriptive test names (test_plugin_processes_csv_data)
   - Include docstrings
   - Test both success and error cases if possible
   - Use pytest assertions (assert, pytest.raises)

4. Import necessary modules:
   - from casparian_flow.engine.context import WorkerContext
   - from casparian_flow.engine.config import StorageConfig
   - Any other required imports

5. DO NOT include:
   - Time estimates or implementation timelines
   - Placeholder TODOs
   - Comments about "this should take X time"

Please generate ONLY the Python test code, wrapped in a ```python code fence.
"""
        return prompt

    def _extract_code_from_markdown(self, response: str) -> str:
        """Extract Python code from markdown fence."""
        # Look for ```python ... ``` or ``` ... ```
        lines = response.split("\n")
        code_lines = []
        in_fence = False
        fence_lang = None

        for line in lines:
            if line.strip().startswith("```"):
                if not in_fence:
                    # Starting fence
                    in_fence = True
                    fence_lang = line.strip()[3:].strip()
                else:
                    # Ending fence
                    break
            elif in_fence:
                code_lines.append(line)

        if code_lines:
            return "\n".join(code_lines)

        # Fallback: return entire response if no fence found
        return response

    def validate_test_syntax(self, test_code: str) -> bool:
        """Verify test code is valid Python."""
        try:
            ast.parse(test_code)
            return True
        except SyntaxError as e:
            logger.error(f"Test code syntax error: {e}")
            return False

    def write_test_file(self, test_code: str, plugin_name: str) -> str:
        """Write test to tests/generated/ directory."""
        # Create tests/generated directory if it doesn't exist
        test_dir = Path("tests/generated")
        test_dir.mkdir(parents=True, exist_ok=True)

        # Generate test file path
        test_file_path = test_dir / f"test_{plugin_name}.py"

        # Write test code
        test_file_path.write_text(test_code, encoding="utf-8")

        logger.info(f"Test written to {test_file_path}")

        return str(test_file_path)
