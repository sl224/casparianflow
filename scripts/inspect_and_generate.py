
"""
CLI Tool: inspect_and_generate.py
Orchestrates the AI Plugin Generation Workflow.
Usage: python inspect_and_generate.py <path_to_data_file> --topic <topic_name>

Flow:
1. Inspector -> FileProfile
2. Generator -> SchemaProposal
3. User (CLI) -> Approve/Deny
4. Generator -> PluginCode
5. Disk -> Write file (triggering Scout)
"""
import argparse
import sys
import json
from pathlib import Path

# Add src to path
sys.path.append(str(Path.cwd() / "src"))

from casparian_flow.services.ai_types import FileProfile, SchemaProposal, PluginCode
from casparian_flow.services.inspector import profile_file
from casparian_flow.services.ai_hook import MockGenerator, AIGenerator

def print_header(msg):
    print(f"\n{'='*60}\n{msg}\n{'='*60}")

def interact_with_user(proposal: SchemaProposal) -> bool:
    """
    Present the proposal (POD) to the user and request approval.
    True = Approved, False = Denied.
    """
    print_header("SCHEMA PROPOSAL")
    print(f"Inferred File Type: {proposal.file_type_inferred}")
    print(f"Target Topic:       {proposal.target_topic}")
    print(f"Read Strategy:      {proposal.read_strategy}")
    print("-" * 20)
    print("Columns:")
    for col in proposal.columns:
        print(f"  - {col.name:<15} ({col.target_type})")
    print("-" * 20)
    print(f"Reasoning: {proposal.reasoning}")
    
    while True:
        choice = input("\nApprove this schema? [y/N]: ").strip().lower()
        if choice in ['y', 'yes']:
            return True
        if choice in ['n', 'no', '']:
            return False

def save_plugin(code: PluginCode, output_dir: Path, sign: bool = False):
    """
    Write the PluginCode POD to disk.
    If sign=True, also generate a .sig file.
    """
    if not output_dir.exists():
        output_dir.mkdir(parents=True)
        
    target_path = output_dir / code.filename
    sig_path = output_dir / (code.filename + ".sig")
    
    # Write Source
    with open(target_path, "w", encoding="utf-8") as f:
        f.write(code.source_code)
        
    if sign:
        from casparian_flow.security.signing import Signer
        signature = Signer.sign(code.source_code)
        
        with open(sig_path, "w", encoding="utf-8") as f:
            f.write(signature)
        print(f"[SIGNED] Signature written to: {sig_path}")
        
    print(f"\n[SUCCESS] Plugin written to: {target_path}")
    print("Scout should detect this file shortly.")

from casparian_flow.services.llm_provider import get_provider
from casparian_flow.services.llm_generator import LLMGenerator

def main():
    parser = argparse.ArgumentParser(description="AI Plugin Generator")
    parser.add_argument("file", help="Path to data file to inspect")
    parser.add_argument("--output-dir", default="plugins/generated", help="Where to save the plugin")
    parser.add_argument("--mock", action="store_true", help="Use Mock Generator instead of LLM")
    parser.add_argument("--provider", default="openai", help="LLM Provider (openai, anthropic, gemini)")
    parser.add_argument("--model", default=None, help="Override default model for provider")
    
    args = parser.parse_args()
    
    target_path = Path(args.file)
    if not target_path.exists():
        print(f"[ERROR] File not found: {target_path}")
        sys.exit(1)
        
    # 1. Inspect (Stateless)
    print(f"Inspecting {target_path}...")
    try:
        profile = profile_file(str(target_path))
    except Exception as e:
        print(f"[FATAL] Inspection failed: {e}")
        sys.exit(1)
        
    print(f"Detected: {profile.file_type.name} ({profile.total_size} bytes)")
    
    # 2. Select Generator
    if args.mock:
        print("Using Mock Generator.")
        generator: AIGenerator = MockGenerator()
    else:
        # Load Real Provider
        try:
            print(f"Initializing {args.provider.upper()} provider...")
            provider = get_provider(args.provider, default_model=args.model)
            generator = LLMGenerator(provider)
        except Exception as e:
            print(f"[FATAL] Failed to initialize LLM provider: {e}")
            sys.exit(1)
    
    # 3. Propose Phase
    print("Generating schema proposal...")
    try:
        proposal = generator.propose_schema(profile)
    except Exception as e:
        print(f"[ERROR] Proposal generation failed: {e}")
        sys.exit(1)
    
    # 4. User Review
    if not interact_with_user(proposal):
        print("\n[ABORTED] Proposal denied by user.")
        sys.exit(0)
        
    # 5. Generate Phase
    print("\nGenerating plugin code...")
    plugin_code = generator.generate_plugin(proposal)
    
    # 6. Save (Plugin-as-Data)
    # The user already approved the Schema. 
    # Implicitly, writing it to disk IS the approval.
    # But let's be explicit about signing.
    save_plugin(plugin_code, Path(args.output_dir), sign=True)

if __name__ == "__main__":
    main()
