
"""
AIGenerator Interface.
Contract for LLM/Agent interactions.
"""
from abc import ABC, abstractmethod
from typing import Optional

from casparian_flow.services.ai_types import FileProfile, SchemaProposal, PluginCode

class AIGenerator(ABC):
    """
    Abstract Base Class for AI Code Generation.
    Implementations (Mock, MCP, OpenAI) must strictly adhere to this contract.
    """
    
    @abstractmethod
    def propose_schema(self, profile: FileProfile) -> SchemaProposal:
        """
        Step 1: Inspect profile and propose an intent/schema.
        """
        pass
        
    @abstractmethod
    def generate_plugin(self, proposal: SchemaProposal) -> PluginCode:
        """
        Step 2: Generate code based on APPROVED proposal.
        """
        pass

class MockGenerator(AIGenerator):
    """
    Reference Implementation (No-Op).
    Useful for testing the plumbing without an LLM.
    """
    def propose_schema(self, profile: FileProfile) -> SchemaProposal:
        from casparian_flow.services.ai_types import ColumnDef, FileType
        
        # Simple heuristic for mock
        is_csv = (profile.file_type == FileType.TEXT_CSV)
        
        return SchemaProposal(
            file_type_inferred=profile.file_type.name,
            target_topic="generated_output",
            columns=[
                ColumnDef(name="col_1", target_type="string"),
                ColumnDef(name="col_2", target_type="int")
            ],
            read_strategy="pandas" if is_csv else "manual",
            reasoning="Mock Reasoning: Detected Text content."
        )

    def generate_plugin(self, proposal: SchemaProposal) -> PluginCode:
        code = f"""
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    # Generated from Proposal: {proposal.reasoning}
    def execute(self, path: str):
        # Strategy: {proposal.read_strategy}
        pass
"""
        return PluginCode(
            filename="generated_plugin.py",
            source_code=code,
            imports=["pandas", "casparian_flow"],
        )
