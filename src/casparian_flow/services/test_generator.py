# src/casparian_flow/services/test_generator.py
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
    test_file_path: str
    test_code: str
    success: bool
    error_message: Optional[str] = None

class TestGenerator:
    def __init__(self, llm_generator: LLMGenerator):
        self.llm_generator = llm_generator

    def generate_test(
        self,
        plugin_manifest: PluginManifest,
        schema_proposal: SchemaProposal,
        sample_file: FileLocation,
    ) -> GeneratedTest:
        logger.info(f"Generating test for plugin {plugin_manifest.plugin_name}")

        prompt = self._build_test_prompt(
            plugin_manifest.source_code,
            schema_proposal,
            sample_file.rel_path,
        )

        try:
            response = self.llm_generator.provider.chat_completion(
                messages=[{"role": "user", "content": prompt}],
                model=self.llm_generator.provider.default_model,
                json_mode=False,
            )

            test_code = self._extract_code_from_markdown(response)

            if not self.validate_test_syntax(test_code):
                return GeneratedTest("", test_code, False, "Invalid Python syntax")

            test_file_path = self.write_test_file(test_code, plugin_manifest.plugin_name)
            logger.info(f"Test generated successfully: {test_file_path}")

            return GeneratedTest(test_file_path, test_code, True, None)

        except Exception as e:
            logger.exception(f"Test generation failed: {e}")
            return GeneratedTest("", "", False, str(e))

    def _build_test_prompt(
        self,
        plugin_code: str,
        schema: SchemaProposal,
        sample_path: str,
    ) -> str:
        
        # Build schema description from tables list
        schema_desc = f"File Type: {schema.file_type_inferred}\nRead Strategy: {schema.read_strategy}\n"
        for t in schema.tables:
            schema_desc += f"\nTable: {t.topic_name}\nColumns: "
            col_names = [c.name for c in t.columns]
            schema_desc += ", ".join(col_names) + "\n"

        # Note: We use triple quotes carefully to ensure valid string formatting
        prompt = f"""Generate a pytest test file for the following plugin code.

PLUGIN CODE:
```python
{plugin_code}

```

EXPECTED SCHEMA:
{schema_desc}

SAMPLE FILE PATH: {sample_path}

REQUIREMENTS:

1. Use existing pytest fixtures: test_db_engine, test_db_session, test_source_root, temp_test_dir.
2. Create sample input data.
3. Verify output matches expected schema for ALL tables defined.
4. Output ONLY Python code.
"""
        return prompt

    def _extract_code_from_markdown(self, response: str) -> str:
        lines = response.split("\n")
        code_lines = []
        in_fence = False
        for line in lines:
            if line.strip().startswith("```"):
                in_fence = not in_fence
            elif in_fence:
                code_lines.append(line)
        # If no markdown fences were found, assume the whole response is code (or return as is)
        return "\n".join(code_lines) if code_lines else response

    def validate_test_syntax(self, test_code: str) -> bool:
        try:
            ast.parse(test_code)
            return True
        except SyntaxError:
            return False

    def write_test_file(self, test_code: str, plugin_name: str) -> str:
        test_dir = Path("tests/generated")
        test_dir.mkdir(parents=True, exist_ok=True)
        path = test_dir / f"test_{plugin_name}.py"
        path.write_text(test_code, encoding="utf-8")
        return str(path)
