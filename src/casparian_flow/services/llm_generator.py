# src/casparian_flow/services/llm_generator.py
import json
import logging
import re
from typing import List, Optional, Type, TypeVar
from pydantic import BaseModel, Field

from casparian_flow.services.ai_hook import AIGenerator
from casparian_flow.services.ai_types import (
    FileProfile, 
    SchemaProposal, 
    PluginCode, 
    ColumnDef, 
    TableDefinition
)
from casparian_flow.services.llm_provider import LLMProvider

logger = logging.getLogger(__name__)

T = TypeVar("T", bound=BaseModel)

# --- Pydantic Models for Structured Output ---
# These act as the "Spec" for the LLM.
class ColumnModel(BaseModel):
    name: str
    target_type: str = Field(description="int, float, string, bool, or datetime")
    description: Optional[str] = None

class TableModel(BaseModel):
    topic_name: str = Field(description="clean_snake_case name")
    description: str
    columns: List[ColumnModel]

class SchemaResponseModel(BaseModel):
    file_type_inferred: str
    tables: List[TableModel]
    read_strategy: str = Field(description="pandas, pyarrow, json, or custom")
    reasoning: str

# ---------------------------------------------

class LLMGenerator(AIGenerator):
    def __init__(self, provider: LLMProvider):
        self.provider = provider
        
    def propose_schema(self, profile: FileProfile, user_feedback: Optional[str] = None) -> SchemaProposal:
        """
        Step 1: Infer schema. Uses Pydantic for schema definition and validation.
        """
        # 1. Prepare Context
        sample_str = self._decode_sample(profile)
        schema_json = json.dumps(SchemaResponseModel.model_json_schema(), indent=2)

        # 2. Build Prompt
        system_prompt = self._get_schema_system_prompt(schema_json)
        user_prompt = self._get_schema_user_prompt(profile, sample_str, user_feedback)
        
        logger.info("Sending PROPOSE request to LLM...")
        resp_str = self.provider.chat_completion(
            messages=[
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            json_mode=True
        )
        
        # 3. Parse & Validate with Pydantic
        try:
            # Still need robust extraction because providers are chatty
            json_str = self._extract_json(resp_str)
            parsed = SchemaResponseModel.model_validate_json(json_str)
            
            # Convert Pydantic Model -> Internal POD (Domain Object)
            # This decouples the AI layer from the Core layer
            tables = []
            for t in parsed.tables:
                cols = [
                    ColumnDef(name=c.name, target_type=c.target_type, description=c.description) 
                    for c in t.columns
                ]
                tables.append(TableDefinition(
                    topic_name=t.topic_name, 
                    columns=cols, 
                    description=t.description
                ))
            
            return SchemaProposal(
                file_type_inferred=parsed.file_type_inferred,
                tables=tables,
                read_strategy=parsed.read_strategy,
                reasoning=parsed.reasoning
            )
            
        except Exception as e:
            logger.error(f"Failed to parse LLM response: {resp_str}")
            raise ValueError(f"LLM returned invalid structure: {e}")

    def generate_plugin(self, proposal: SchemaProposal, user_feedback: Optional[str] = None, example_path: str = "") -> PluginCode:
        """
        Step 2: Generate Code. 
        Strings are fine here since we want code, not data.
        """
        system_prompt = self._get_code_system_prompt()
        user_prompt = self._get_code_user_prompt(proposal, user_feedback, example_path)

        logger.info("Sending GENERATE request to LLM...")
        code_str = self.provider.chat_completion(
             messages=[
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            json_mode=False
        )
        
        code_str = self._clean_markdown(code_str)
        primary_topic = proposal.tables[0].topic_name if proposal.tables else "output"
             
        return PluginCode(
            filename=f"generated_{primary_topic}.py",
            source_code=code_str,
            imports=[], 
            entry_point="Handler"
        )

    # --- Helpers & Prompts ---

    def _decode_sample(self, profile: FileProfile) -> str:
        if not profile.head_sample.encoding_detected:
            return "<binary_data>"
        try:
            return profile.head_sample.data.decode(
                profile.head_sample.encoding_detected, errors="replace"
            )[:3000]
        except Exception:
            return "<binary_decode_error>"

    def _extract_json(self, text: str) -> str:
        """Robust JSON extractor."""
        text = text.strip()
        if text.startswith("{") and text.endswith("}"):
            return text
        match = re.search(r"(\{.*\})", text, re.DOTALL)
        return match.group(1) if match else text

    def _clean_markdown(self, text: str) -> str:
        """Robust code fence stripper."""
        text = text.strip()
        patterns = [r"```python\s*(.*?)```", r"```\s*(.*?)```"]
        for p in patterns:
            match = re.search(p, text, re.DOTALL)
            if match:
                return match.group(1).strip()
        return text

    # --- Prompts ---

    def _get_schema_system_prompt(self, json_schema: str) -> str:
        return f"""
You are a Senior Data Engineer. 
Analyze the file sample and propose a data schema.

CRITICAL RULES:
1. **Logic Split**: Look for discriminator columns (e.g. Record Type). If found, split into multiple tables.
2. **Structure**: Infer headers if missing.

You must output ONLY valid JSON that complies with this JSON Schema:
{json_schema}
"""

    def _get_schema_user_prompt(self, profile, sample_str, feedback) -> str:
        prompt = f"""
File Path: {profile.path}
Metadata Hints: {profile.metadata_hints}

Sample Data:

{sample_str}

"""
        if feedback:
            prompt += f"\nUSER FEEDBACK (Override previous assumptions):\n'{feedback}'\n"
        return prompt

    def _get_code_system_prompt(self) -> str:
        return """
You are a Python Expert. Write a Casparian Flow Plugin.

Rules:
1. Define `MANIFEST = PluginMetadata(subscriptions=["input_topic"])`. NO pattern.
2. Implement `consume(self, event: FileEvent)`. Access path via `event.path`.
3. Use `self.publish(topic, df)` for EACH table.
4. Output ONLY Python code.
"""

    def _get_code_user_prompt(self, proposal, feedback, example_path) -> str:
        filename = re.split(r'[/\\]', example_path)[-1] if example_path else "*.csv"
        
        schema_desc = f"Goal: Read format '{proposal.file_type_inferred}'\nTables:"
        topics = []
        for t in proposal.tables:
            topics.append(t.topic_name)
            schema_desc += f"\n- Table '{t.topic_name}': {t.description}"
            schema_desc += "\n  Cols: " + ", ".join([f"{c.name}({c.target_type})" for c in t.columns])

        prompt = f"""
{schema_desc}

Reasoning: {proposal.reasoning}

Constraint: The MANIFEST pattern/subscription must target files like '{filename}'.
"""
        if feedback:
            prompt += f"\nUSER FEEDBACK:\n'{feedback}'\n"
            
        return prompt + "\nWrite the handler code."