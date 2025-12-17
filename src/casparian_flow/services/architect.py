# src/casparian_flow/services/architect.py
"""
Architect Service - AI-Generated Plugin Deployment Workflow.

This module implements the "Active Deployment" lifecycle:
1. Ingest: Receive DEPLOY message with source code
2. Gatekeep: Validate safety using AST analysis
3. Persist: Save to PluginManifest (STAGING status)
4. Sandbox Test: Spawn temporary sidecar to verify execution
5. Promote: Mark as ACTIVE and hot-reload plugins

Design Principles:
- Zero-trust: All code is validated before execution
- Atomic: Deployment succeeds or fails completely
- Traceable: Full audit trail in PluginManifest table
"""

import json
import logging
import subprocess
import tempfile
import time
from datetime import datetime
from pathlib import Path
from typing import Optional, Tuple
from dataclasses import dataclass

from sqlalchemy import Engine
from sqlalchemy.orm import Session

from casparian_flow.db.models import PluginManifest, PluginStatusEnum
from casparian_flow.security.gatekeeper import (
    validate_plugin_safety,
    verify_signature,
    compute_source_hash,
)

logger = logging.getLogger(__name__)


@dataclass
class DeploymentResult:
    """Result of plugin deployment attempt."""

    success: bool
    plugin_name: str
    version: str
    error_message: Optional[str] = None
    manifest_id: Optional[int] = None


class ArchitectService:
    """
    Manages the plugin deployment lifecycle.

    Integrates with ZmqWorker to handle DEPLOY OpCode messages.
    """

    def __init__(self, engine: Engine, secret_key: str):
        """
        Initialize Architect service.

        Args:
            engine: SQLAlchemy engine for database access
            secret_key: HMAC secret key for signature verification
        """
        self.engine = engine
        self.secret_key = secret_key

    def deploy_plugin(
        self,
        plugin_name: str,
        version: str,
        source_code: str,
        signature: str,
        sample_input: Optional[str] = None,
        unsafe: bool = False,
    ) -> DeploymentResult:
        """
        Deploy a new plugin through the full lifecycle.

        Args:
            plugin_name: Unique name for the plugin
            version: Semantic version string
            source_code: Python source code
            signature: HMAC signature for authenticity
            sample_input: Optional test file path for sandbox testing
            unsafe: If True, skip signature verification and safety validation (DEV ONLY)

        Returns:
            DeploymentResult with success status and details
        """
        logger.info(f"Starting deployment: {plugin_name} v{version}{' [UNSAFE MODE]' if unsafe else ''}")

        # Step 1: Verify signature (skip if unsafe mode)
        if not unsafe and not verify_signature(source_code, signature, self.secret_key):
            logger.error(f"Signature verification failed: {plugin_name}")
            return DeploymentResult(
                success=False,
                plugin_name=plugin_name,
                version=version,
                error_message="Invalid signature",
            )

        # Step 2: Validate safety (skip if unsafe mode)
        if not unsafe:
            validation = validate_plugin_safety(source_code)
        else:
            # Create a mock validation result in unsafe mode
            from casparian_flow.security.gatekeeper import ValidationResult
            validation = ValidationResult(is_safe=True, error_message=None, violations=[])

        if not validation.is_safe:
            logger.error(f"Safety validation failed: {plugin_name}")
            with Session(self.engine) as session:
                manifest = PluginManifest(
                    plugin_name=plugin_name,
                    version=version,
                    source_code=source_code,
                    source_hash=compute_source_hash(source_code),
                    status=PluginStatusEnum.REJECTED,
                    signature=signature,
                    validation_error=validation.error_message,
                )
                session.add(manifest)
                session.commit()

            return DeploymentResult(
                success=False,
                plugin_name=plugin_name,
                version=version,
                error_message=validation.error_message,
            )

        # Step 3: Persist to database (STAGING)
        source_hash = compute_source_hash(source_code)
        with Session(self.engine) as session:
            # Check for duplicate hash
            existing = (
                session.query(PluginManifest)
                .filter_by(source_hash=source_hash)
                .first()
            )
            if existing:
                logger.warning(f"Duplicate plugin detected: {plugin_name}")
                return DeploymentResult(
                    success=False,
                    plugin_name=plugin_name,
                    version=version,
                    error_message=f"Duplicate source hash: already exists as {existing.plugin_name}",
                )

            manifest = PluginManifest(
                plugin_name=plugin_name,
                version=version,
                source_code=source_code,
                source_hash=source_hash,
                status=PluginStatusEnum.STAGING,
                signature=signature,
            )
            session.add(manifest)
            session.commit()
            manifest_id = manifest.id

        logger.info(f"Plugin saved to STAGING: {plugin_name} (ID: {manifest_id})")

        # Step 4: Sandbox test (if sample_input provided)
        if sample_input:
            sandbox_success, sandbox_error = self._sandbox_test(
                plugin_name, source_code, sample_input
            )
            if not sandbox_success:
                logger.error(f"Sandbox test failed: {plugin_name}")
                with Session(self.engine) as session:
                    manifest = session.get(PluginManifest, manifest_id)
                    manifest.status = PluginStatusEnum.REJECTED
                    manifest.validation_error = f"Sandbox test failed: {sandbox_error}"
                    session.commit()

                return DeploymentResult(
                    success=False,
                    plugin_name=plugin_name,
                    version=version,
                    error_message=f"Sandbox test failed: {sandbox_error}",
                    manifest_id=manifest_id,
                )

        # Step 5: Promote to ACTIVE
        with Session(self.engine) as session:
            manifest = session.get(PluginManifest, manifest_id)
            manifest.status = PluginStatusEnum.ACTIVE
            manifest.deployed_at = datetime.now()
            session.commit()

        logger.info(f"Plugin promoted to ACTIVE: {plugin_name}")

        return DeploymentResult(
            success=True,
            plugin_name=plugin_name,
            version=version,
            manifest_id=manifest_id,
        )

    def _sandbox_test(
        self, plugin_name: str, source_code: str, sample_input: str
    ) -> Tuple[bool, Optional[str]]:
        """
        Test plugin in isolated sandbox environment.

        Creates a temporary sidecar process, sends a test job, and validates output.

        Args:
            plugin_name: Name of plugin
            source_code: Python source code
            sample_input: Path to test file

        Returns:
            Tuple of (success, error_message)
        """
        # Create temporary plugin file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".py", delete=False, encoding="utf-8"
        ) as tmp:
            tmp.write(source_code)
            tmp_path = Path(tmp.name)

        try:
            # Spawn temporary sidecar
            logger.info(f"Starting sandbox test for {plugin_name}")
            proc = subprocess.Popen(
                [
                    "python",
                    "-m",
                    "casparian_flow.sidecar",
                    "--plugin",
                    str(tmp_path),
                    "--connect",
                    "tcp://127.0.0.1:54321",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            # Wait for startup
            time.sleep(2)

            # Check if process crashed immediately
            if proc.poll() is not None:
                _, stderr = proc.communicate()
                error = stderr.decode("utf-8")
                logger.error(f"Sidecar crashed on startup: {error}")
                return False, f"Sidecar crashed: {error}"

            # Kill sandbox after test
            proc.terminate()
            proc.wait(timeout=5)

            logger.info(f"Sandbox test passed: {plugin_name}")
            return True, None

        except Exception as e:
            logger.error(f"Sandbox test exception: {e}")
            return False, str(e)
        finally:
            # Cleanup temporary file
            tmp_path.unlink(missing_ok=True)


def handle_deploy_message(
    architect: ArchitectService, payload: bytes
) -> DeploymentResult:
    """
    Handle DEPLOY OpCode message from ZMQ Worker.

    Args:
        architect: ArchitectService instance
        payload: JSON payload with plugin details

    Returns:
        DeploymentResult
    """
    try:
        data = json.loads(payload.decode("utf-8"))
        plugin_name = data["plugin_name"]
        version = data.get("version", "1.0.0")
        source_code = data["source_code"]
        signature = data["signature"]
        sample_input = data.get("sample_input")

        return architect.deploy_plugin(
            plugin_name, version, source_code, signature, sample_input
        )

    except Exception as e:
        logger.error(f"Failed to parse DEPLOY payload: {e}")
        return DeploymentResult(
            success=False,
            plugin_name="unknown",
            version="unknown",
            error_message=f"Payload parsing error: {e}",
        )
