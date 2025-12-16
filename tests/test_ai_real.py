
"""
Real Integration Tests for AI Components.
These tests make ACTUAL API calss to OpenAI (or other providers).
They are skipped if no API key is present.
"""
import pytest
import os
from casparian_flow.services.ai_types import FileProfile, FileType, HEAD_Sample
from casparian_flow.services.llm_provider import OpenAIProvider, get_provider
from casparian_flow.services.llm_generator import LLMGenerator

# Check for API Key
HAS_OPENAI_KEY = bool(os.environ.get("OPENAI_API_KEY"))

@pytest.fixture
def real_csv_profile(tmp_path):
    # Determine profile manually to avoid IO in this test
    return FileProfile(
        path="real_sales_data.csv",
        file_type=FileType.TEXT_CSV,
        total_size=1024,
        head_sample=HEAD_Sample(
            data=b"id,product,amount,date\n101,Widget,50.5,2023-01-01\n102,Gadget,20.0,2023-01-02",
            encoding_detected="utf-8"
        ),
        metadata_hints={}
    )

@pytest.mark.skipif(not HAS_OPENAI_KEY, reason="OPENAI_API_KEY not set")
def test_openai_propose_schema(real_csv_profile):
    """
    Integration: Call GPT-4o to propose a schema.
    """
    provider = get_provider("openai")
    generator = LLMGenerator(provider)
    
    print(f"\n[INTEGRATION] Calling {provider.default_model}...")
    proposal = generator.propose_schema(real_csv_profile)
    
    print(f"\n[Result] Reasoning: {proposal.reasoning}")
    
    assert proposal.file_type_inferred.upper() in ["CSV", "TEXT_CSV"]
    assert len(proposal.columns) == 4
    # Check column names match sample
    names = [c.name for c in proposal.columns]
    assert "product" in names
    assert "amount" in names

@pytest.mark.skipif(not HAS_OPENAI_KEY, reason="OPENAI_API_KEY not set")
def test_openai_generate_code(real_csv_profile):
    """
    Integration: Call GPT-4o to generate Python code.
    """
    provider = get_provider("openai")
    generator = LLMGenerator(provider)
    
    # 1. Propose
    proposal = generator.propose_schema(real_csv_profile)
    
    # 2. Generate
    plugin_code = generator.generate_plugin(proposal)
    
    print(f"\n[Result] Generated {len(plugin_code.source_code)} bytes of code.")
    
    assert "class Handler" in plugin_code.source_code
    assert "BasePlugin" in plugin_code.source_code
    assert "def execute" in plugin_code.source_code
    
    # 3. Compilation check
    compile(plugin_code.source_code, "gen.py", "exec")
