# src/casparian_flow/cli/publish.py
"""
v5.0 Bridge Mode: Artifact Publishing CLI.

The `casparian publish` command implements the Publisher workflow:
1. Discovery: User points to plugin.py, CLI looks for pyproject.toml or uv.lock
2. Locking: Runs `uv lock --universal` for cross-platform compatibility
3. Hashing: Computes env_hash and artifact_hash
4. Authentication: Local API key or Azure AD Device Code Flow
5. Upload: Sends OpCode.DEPLOY to Sentinel

Usage:
    casparian publish my_plugin.py
    casparian publish my_plugin.py --version 1.0.0
    casparian publish my_plugin.py --auth entra  # Enterprise mode
"""

import os
import sys
import json
import argparse
import subprocess
import logging
from pathlib import Path
from typing import Optional, Tuple

import zmq

from casparian_flow.security import (
    get_identity_provider,
    compute_artifact_hash,
    compute_env_hash,
    AuthenticationError,
)
from casparian_flow.security.gatekeeper import validate_plugin_safety
from casparian_flow.protocol import (
    OpCode,
    DeployCommand,
    msg_deploy,
    unpack_msg,
    pack_header,
)

logging.basicConfig(level=logging.INFO, format="%(asctime)s [PUBLISH] %(message)s")
logger = logging.getLogger(__name__)


class PublishError(Exception):
    """Raised when publishing fails."""
    pass


def find_lockfile(plugin_path: Path) -> Tuple[Path, str]:
    """
    Find or generate a lockfile for the plugin.

    v5.0 Bridge Mode: Lockfiles are REQUIRED. Auto-generates if missing.

    Discovery order:
    1. uv.lock in same directory as plugin
    2. pyproject.toml in same directory (will run uv lock)
    3. uv.lock in parent directories (up to 3 levels)
    4. Auto-generate minimal pyproject.toml + lockfile

    Returns:
        Tuple of (lockfile_path, lockfile_content)

    Raises:
        PublishError: If lockfile cannot be generated
    """
    plugin_dir = plugin_path.parent

    # Check for existing uv.lock
    lockfile = plugin_dir / "uv.lock"
    if lockfile.exists():
        logger.info(f"Found existing lockfile: {lockfile}")
        return lockfile, lockfile.read_text()

    # Check for pyproject.toml (need to generate lock)
    pyproject = plugin_dir / "pyproject.toml"
    if pyproject.exists():
        logger.info(f"Found pyproject.toml, generating lockfile...")
        return generate_lockfile(plugin_dir)

    # Check parent directories
    for i in range(1, 4):
        parent = plugin_dir.parents[i - 1] if i <= len(plugin_dir.parents) else None
        if parent:
            lockfile = parent / "uv.lock"
            if lockfile.exists():
                logger.info(f"Found lockfile in parent: {lockfile}")
                return lockfile, lockfile.read_text()

    # No lockfile found - auto-generate minimal pyproject.toml
    logger.warning(
        "No lockfile or pyproject.toml found. "
        "Auto-generating minimal pyproject.toml for Bridge Mode..."
    )

    # Create minimal pyproject.toml with common dependencies
    minimal_pyproject = f"""[project]
name = "{plugin_path.stem}"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = [
    "pandas>=2.0.0",
    "pyarrow>=14.0.0",
]
"""
    pyproject_path = plugin_dir / "pyproject.toml"
    pyproject_path.write_text(minimal_pyproject)
    logger.info(f"Created minimal pyproject.toml at {pyproject_path}")

    # Generate lockfile
    return generate_lockfile(plugin_dir)


def generate_lockfile(project_dir: Path) -> Tuple[Path, str]:
    """
    Generate a uv.lock file using `uv lock --universal`.

    The --universal flag ensures cross-platform compatibility.
    """
    try:
        # Check if uv is available
        result = subprocess.run(
            ["uv", "--version"],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise PublishError("uv not found. Install with: curl -LsSf https://astral.sh/uv/install.sh | sh")

        # Run uv lock --universal
        logger.info("Running: uv lock --universal")
        result = subprocess.run(
            ["uv", "lock", "--universal"],
            cwd=project_dir,
            capture_output=True,
            text=True,
            timeout=300,  # 5 minute timeout
        )

        if result.returncode != 0:
            raise PublishError(f"uv lock failed: {result.stderr}")

        lockfile = project_dir / "uv.lock"
        if not lockfile.exists():
            raise PublishError("uv lock completed but no lockfile was created")

        return lockfile, lockfile.read_text()

    except subprocess.TimeoutExpired:
        raise PublishError("uv lock timed out (5 minutes)")
    except FileNotFoundError:
        raise PublishError("uv not found. Install with: curl -LsSf https://astral.sh/uv/install.sh | sh")


def extract_plugin_name(source_code: str, file_path: Path) -> str:
    """Extract plugin name from source code or filename."""
    # Try to find MANIFEST in source
    if "MANIFEST" in source_code:
        import ast
        try:
            tree = ast.parse(source_code)
            for node in ast.walk(tree):
                if isinstance(node, ast.Assign):
                    for target in node.targets:
                        if isinstance(target, ast.Name) and target.id == "MANIFEST":
                            # Try to extract name from PluginMetadata call
                            if isinstance(node.value, ast.Call):
                                for keyword in node.value.keywords:
                                    if keyword.arg == "name":
                                        if isinstance(keyword.value, ast.Constant):
                                            return keyword.value.value
        except:
            pass

    # Fall back to filename
    return file_path.stem


def publish_plugin(
    plugin_path: Path,
    version: str = "1.0.0",
    sentinel_addr: str = "tcp://127.0.0.1:5555",
    auth_mode: Optional[str] = None,
    api_key: Optional[str] = None,
    system_requirements: Optional[list[str]] = None,
    dry_run: bool = False,
) -> dict:
    """
    Publish a plugin to the Casparian registry.

    Args:
        plugin_path: Path to the plugin Python file
        version: Version string
        sentinel_addr: Sentinel ZMQ address
        auth_mode: "local" or "entra"
        api_key: API key for local mode authentication
        system_requirements: List of system requirements (e.g., ["glibc_2.31"])
        dry_run: If True, validate without uploading

    Returns:
        dict with publish results
    """
    plugin_path = Path(plugin_path).resolve()

    if not plugin_path.exists():
        raise PublishError(f"Plugin file not found: {plugin_path}")

    if not plugin_path.suffix == ".py":
        raise PublishError(f"Plugin must be a .py file: {plugin_path}")

    # 1. Read source code
    logger.info(f"Reading plugin: {plugin_path}")
    source_code = plugin_path.read_text()

    # 2. Validate plugin safety
    logger.info("Validating plugin safety...")
    validation = validate_plugin_safety(source_code)
    if not validation.is_safe:
        raise PublishError(f"Plugin validation failed: {validation.error_message}")
    logger.info("✓ Plugin passed safety validation")

    # 3. Find or generate lockfile (required for Bridge Mode)
    lockfile_path, lockfile_content = find_lockfile(plugin_path)

    # 4. Compute hashes
    env_hash = compute_env_hash(lockfile_content)
    artifact_hash = compute_artifact_hash(source_code, lockfile_content)
    logger.info(f"Environment hash: {env_hash[:16]}...")
    logger.info(f"Artifact hash: {artifact_hash[:16]}...")

    # 5. Authenticate
    logger.info("Authenticating...")
    provider = get_identity_provider(mode=auth_mode, api_key=api_key)

    try:
        user = provider.authenticate(api_key)
        logger.info(f"✓ Authenticated as: {user.name}")
    except AuthenticationError as e:
        raise PublishError(f"Authentication failed: {e}")

    # 6. Sign artifact
    logger.info("Signing artifact...")
    signed = provider.sign_artifact(artifact_hash)
    logger.info(f"✓ Artifact signed")

    # 7. Extract plugin name
    plugin_name = extract_plugin_name(source_code, plugin_path)
    logger.info(f"Plugin name: {plugin_name}")

    # Build deploy command
    deploy_cmd = DeployCommand(
        plugin_name=plugin_name,
        version=version,
        source_code=source_code,
        lockfile_content=lockfile_content,
        env_hash=env_hash,
        artifact_hash=artifact_hash,
        signature=signed.signature,
        publisher_name=user.name,
        publisher_email=user.email,
        azure_oid=user.azure_oid,
        system_requirements=system_requirements,
    )

    if dry_run:
        logger.info("Dry run - skipping upload")
        return {
            "status": "DRY_RUN",
            "plugin_name": plugin_name,
            "version": version,
            "artifact_hash": artifact_hash,
            "env_hash": env_hash or None,
            "publisher": user.name,
        }

    # 8. Upload to Sentinel
    logger.info(f"Uploading to Sentinel at {sentinel_addr}...")

    ctx = zmq.Context()
    socket = ctx.socket(zmq.DEALER)
    socket.setsockopt(zmq.LINGER, 0)
    socket.setsockopt(zmq.RCVTIMEO, 30000)  # 30 second timeout

    try:
        socket.connect(sentinel_addr)
        socket.send_multipart(msg_deploy(deploy_cmd))

        # Wait for response
        frames = socket.recv_multipart()
        opcode, job_id, payload = unpack_msg(frames)

        if opcode == OpCode.ERR:
            raise PublishError(f"Sentinel rejected: {payload.get('message')}")

        logger.info("✓ Plugin published successfully!")

        return {
            "status": "SUCCESS",
            "plugin_name": plugin_name,
            "version": version,
            "artifact_hash": artifact_hash,
            "env_hash": env_hash or None,
            "publisher": user.name,
        }

    except zmq.Again:
        raise PublishError("Sentinel connection timed out")
    finally:
        socket.close()
        ctx.term()


def main():
    """CLI entry point for casparian publish."""
    parser = argparse.ArgumentParser(
        description="Publish a plugin to the Casparian registry",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    casparian publish my_plugin.py
    casparian publish my_plugin.py --version 2.0.0
    casparian publish my_plugin.py --auth entra  # Enterprise mode
    casparian publish my_plugin.py --dry-run     # Validate without uploading
        """,
    )

    parser.add_argument(
        "plugin",
        type=Path,
        help="Path to the plugin Python file",
    )
    parser.add_argument(
        "--version", "-v",
        default="1.0.0",
        help="Plugin version (default: 1.0.0)",
    )
    parser.add_argument(
        "--sentinel",
        default="tcp://127.0.0.1:5555",
        help="Sentinel address (default: tcp://127.0.0.1:5555)",
    )
    parser.add_argument(
        "--auth",
        choices=["local", "entra"],
        default=None,
        help="Authentication mode (default: from AUTH_MODE env)",
    )
    parser.add_argument(
        "--api-key",
        default=None,
        help="API key for local authentication",
    )
    parser.add_argument(
        "--requirements",
        nargs="*",
        default=None,
        help="System requirements (e.g., glibc_2.31 cuda_11.8)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate without uploading",
    )

    args = parser.parse_args()

    try:
        result = publish_plugin(
            plugin_path=args.plugin,
            version=args.version,
            sentinel_addr=args.sentinel,
            auth_mode=args.auth,
            api_key=args.api_key,
            system_requirements=args.requirements,
            dry_run=args.dry_run,
        )

        print("\n" + "=" * 50)
        print("PUBLISH RESULT")
        print("=" * 50)
        for key, value in result.items():
            print(f"  {key}: {value}")
        print("=" * 50)

        sys.exit(0)

    except PublishError as e:
        logger.error(f"Publish failed: {e}")
        sys.exit(1)
    except Exception as e:
        logger.error(f"Unexpected error: {e}", exc_info=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
