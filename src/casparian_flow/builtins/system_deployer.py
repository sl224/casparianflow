"""
System Plugin: SystemDeployer
Automates the 'Plugin-as-Data' workflow.
Watches for new python files in the plugins directory and deploys them via the Architect.

Data-Oriented Design:
- stateless execute()
- explicit dependency on 'ArchitectService' (via params or context)
"""

from pathlib import Path
from casparian_flow.sdk import BasePlugin
from casparian_flow.services.architect import ArchitectService
from casparian_flow.security.signing import Signer
from casparian_flow.security.gatekeeper import verify_signature


class Handler(BasePlugin):
    def configure(self, ctx, config: dict):
        self.ctx = ctx
        self.architect_secret = config.get(
            "architect_secret", "default-secret-key-change-me"
        )

    def execute(self, file_path: str):
        """
        Triggered when a new .py file is found in 'plugins/generated'
        """
        p = Path(file_path)
        if p.suffix != ".py":
            return

        print(f"[SystemDeployer] Detected new plugin source: {p.name}")

        # 1. Read Code
        with open(p, "r", encoding="utf-8") as f:
            source_code = f.read()

        plugin_name = p.stem  # e.g. "generated_plugin"

        # 3. Verify Signature (Registration Check)
        # We look for a corresponding .sig file which proves the user (via CLI)
        # explicitly approved this code.
        sig_path = p.with_suffix(p.suffix + ".sig")

        if not sig_path.exists():
            print(
                f"[SystemDeployer] SKIPPING {p.name}: No signature file found (not registered)."
            )
            return

        with open(sig_path, "r", encoding="utf-8") as f:
            registered_sig = f.read().strip()

        # Verify HMAC signature using the architect secret key
        if not verify_signature(source_code, registered_sig, self.architect_secret):
            print(
                f"[SystemDeployer] ALARM {p.name}: Signature mismatch! Possible tampering."
            )
            return

        print(f"[SystemDeployer] Verified signature for {p.name}. Deploying...")
        sig = registered_sig  # Use the verified HMAC signature for deployment

        # 4. Deploy via ZMQ Protocol
        self.ctx.send_deploy(
            plugin_name=plugin_name,
            version="1.0.0",
            source_code=source_code,
            signature=sig,
        )
