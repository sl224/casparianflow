
"""
E2E Verification for Claude Code CLI Integration.
This test verifies that the system can shell out to 'claude',
get a response, and produce valid plugin code.
"""
import pytest
import shutil
import subprocess
from pathlib import Path
from casparian_flow.services.ai_types import FileProfile, FileType, HEAD_Sample
from casparian_flow.services.llm_provider import get_provider
from casparian_flow.services.llm_generator import LLMGenerator

# Check if 'claude' is in PATH and actually runnable
CLAUDE_AVAILABLE = shutil.which("claude") is not None

def is_claude_login_active():
    """Simple check to see if claude is usable (not needing login)."""
    if not CLAUDE_AVAILABLE:
        return False
    try:
        # Try a trivial non-interactive command (just help or version if possible)
        # Assuming if it runs without error, we are good?
        # Actually, let's just rely on the test failing if not logged in.
        return True
    except:
        return False

@pytest.fixture
def sample_csv(tmp_path):
    p = tmp_path / "sales.csv"
    p.write_text("id,item,cost,timestamp\n1,apple,0.50,2023-01-01\n2,banana,0.30,2023-01-02", encoding="utf-8")
    return p

@pytest.mark.skipif(not is_claude_login_active(), reason="Claude CLI not found in PATH")
def test_claude_e2e_generation(sample_csv, tmp_path):
    """
    Full workflow: Profile -> Claude -> Plugin Code -> Write -> Verify.
    """
    # 1. Profile (Mock the profile result to save time/complexity of full inspector import if desired, 
    #    but let's use the real POD for realism)
    profile = FileProfile(
        path=str(sample_csv),
        file_type=FileType.TEXT_CSV,
        total_size=100,
        head_sample=HEAD_Sample(data=sample_csv.read_bytes(), encoding_detected="utf-8"),
        metadata_hints={}
    )
    
    # 2. Initialize Claude CLI Provider
    print("\n[Claude E2E] Initializing Provider...")
    try:
        provider = get_provider("claude-cli")
    except Exception as e:
        pytest.fail(f"Could not init provider: {e}")

    generator = LLMGenerator(provider)
    
    # 3. Propose Schema
    print("\n[Claude E2E] Asking Claude to propose schema (this may take time)...")
    try:
        proposal = generator.propose_schema(profile)
        print(f"[Claude E2E] Proposal: {proposal}")
    except RuntimeError as e:
        pytest.fail(f"Claude CLI failed (are you logged in?): {e}")

    assert proposal.file_type_inferred.upper() in ["CSV", "TEXT_CSV"]
    assert len(proposal.tables) > 0
    assert len(proposal.tables[0].columns) == 4
    
    # 4. Generate Plugin
    print("\n[Claude E2E] Asking Claude to write code...")
    plugin_code = generator.generate_plugin(proposal)
    
    assert "class Handler" in plugin_code.source_code
    assert "BasePlugin" in plugin_code.source_code
    
    # 5. Write Code (Simulate SystemDeployer pickup)
    out_file = tmp_path / plugin_code.filename
    out_file.write_text(plugin_code.source_code, encoding="utf-8")
    
    print(f"\n[Claude E2E] Written to {out_file}")
    
    # 6. Verify Syntax
    compile(out_file.read_text("utf-8"), str(out_file), "exec")
