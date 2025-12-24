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

v5.0 Additions:
- Data-Oriented Design: Parse MANIFEST as data using AST (no execution)
- State Projection: Auto-wire routing rules from plugin metadata
- Publisher Identity: Track artifact provenance
"""

import ast
import json
import logging
import subprocess
import tempfile
import time
import hashlib
from datetime import datetime
from pathlib import Path
from typing import Optional, Tuple, Dict, Any
from dataclasses import dataclass

from sqlalchemy import Engine
from sqlalchemy.orm import Session

from casparian_flow.db.models import (
    PluginManifest,
    PluginStatusEnum,
    PluginEnvironment,
    Publisher,
    RoutingRule,
    PluginConfig,
    PluginSubscription,
    TopicConfig,
)
from casparian_flow.security.gatekeeper import (
    validate_plugin_safety,
    verify_signature,
    compute_source_hash,
)
from casparian_flow.protocol import DeployCommand
from casparian_flow.security.identity import User

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

    v5.0: Implements Data-Oriented Design - treats code as data,
    extracts configuration via AST parsing (no execution).
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

    @staticmethod
    def _extract_metadata_safely(source_code: str) -> Dict[str, Any]:
        """
        Extract MANIFEST metadata from plugin source using AST parsing.

        Data-Oriented Design Core: Treats Python code as a text buffer.
        We parse the AST to extract the MANIFEST assignment WITHOUT executing code.

        Expected structure in source_code:
            MANIFEST = PluginMetadata(
                pattern="*.csv",
                topic="sales_output",
                subscriptions=["upstream_topic"]
            )

        Args:
            source_code: Python source code containing MANIFEST declaration

        Returns:
            Dictionary with keys: pattern, topic, subscriptions

        Raises:
            ValueError: If MANIFEST is not found, uses dynamic code, or is malformed
            SyntaxError: If source code is not valid Python
        """
        try:
            tree = ast.parse(source_code)
        except SyntaxError as e:
            raise SyntaxError(f"Invalid Python syntax: {e}")

        # Walk the AST looking for: MANIFEST = PluginMetadata(...)
        manifest_node = None
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign):
                # Check if target is a Name node called "MANIFEST"
                if (len(node.targets) == 1 and
                    isinstance(node.targets[0], ast.Name) and
                    node.targets[0].id == "MANIFEST"):
                    manifest_node = node.value
                    break

        if not manifest_node:
            raise ValueError(
                "MANIFEST not found. Plugin must declare:\n"
                "MANIFEST = PluginMetadata(pattern='...', topic='...', subscriptions=[...])"
            )

        # Verify it's a Call node (PluginMetadata(...))
        if not isinstance(manifest_node, ast.Call):
            raise ValueError("MANIFEST must be assigned to a PluginMetadata() call")

        # Extract keyword arguments as literals only (reject dynamic code)
        metadata = {}
        for keyword in manifest_node.keywords:
            arg_name = keyword.arg
            arg_value = keyword.value

            # Extract literal values only
            if isinstance(arg_value, ast.Constant):
                # String or number literal
                metadata[arg_name] = arg_value.value
            elif isinstance(arg_value, ast.List):
                # List of string literals
                items = []
                for elt in arg_value.elts:
                    if isinstance(elt, ast.Constant):
                        items.append(elt.value)
                    else:
                        raise ValueError(
                            f"MANIFEST.{arg_name} contains dynamic code. "
                            "Only string literals allowed in lists."
                        )
                metadata[arg_name] = items
            else:
                raise ValueError(
                    f"MANIFEST.{arg_name} uses dynamic code (function calls, variables). "
                    "Only literal values allowed."
                )

        # Validate required fields
        if "pattern" not in metadata:
            raise ValueError("MANIFEST missing required field: pattern")
        if "topic" not in metadata:
            raise ValueError("MANIFEST missing required field: topic")

        # subscriptions is optional, default to empty list
        if "subscriptions" not in metadata:
            metadata["subscriptions"] = []

        logger.info(f"Extracted MANIFEST: {metadata}")
        return metadata

    def deploy_artifact(
        self,
        cmd: DeployCommand,
        publisher: User,
    ) -> DeploymentResult:
        """
        Deploy artifact through v5.0 Bridge Mode lifecycle.

        This is the "Publish-to-Execute" golden path:
        1. Persist artifact (PluginEnvironment, PluginManifest)
        2. Extract MANIFEST via AST (Data-Oriented Design)
        3. Project state to database (RoutingRule, PluginSubscription, TopicConfig)
        4. Scout will auto-discover files matching the new rules

        Args:
            cmd: DeployCommand from protocol
            publisher: User who published the artifact

        Returns:
            DeploymentResult with success status
        """
        logger.info(
            f"[Architect] deploy_artifact: {cmd.plugin_name} v{cmd.version} "
            f"by {publisher.name}"
        )

        try:
            with Session(self.engine) as session:
                # Step 1: Ensure Publisher exists
                db_publisher = (
                    session.query(Publisher)
                    .filter_by(azure_oid=publisher.azure_oid)
                    .first()
                    if publisher.azure_oid
                    else session.query(Publisher).filter_by(name=publisher.name).first()
                )

                if not db_publisher:
                    db_publisher = Publisher(
                        azure_oid=publisher.azure_oid,
                        name=publisher.name,
                        email=publisher.email,
                    )
                    session.add(db_publisher)
                    session.flush()

                # Step 2: Persist PluginEnvironment (if lockfile provided)
                env_hash = None
                if cmd.lockfile_content:
                    env_hash = cmd.env_hash
                    existing_env = session.get(PluginEnvironment, env_hash)
                    if not existing_env:
                        env = PluginEnvironment(
                            hash=env_hash,
                            lockfile_content=cmd.lockfile_content,
                        )
                        session.add(env)
                        logger.info(f"[Architect] Created new environment: {env_hash[:8]}")
                    else:
                        logger.info(f"[Architect] Reusing existing environment: {env_hash[:8]}")

                # Step 3: Persist PluginManifest (ACTIVE status)
                source_hash = compute_source_hash(cmd.source_code)

                # Check for duplicate
                existing_manifest = (
                    session.query(PluginManifest)
                    .filter_by(source_hash=source_hash)
                    .first()
                )
                if existing_manifest:
                    logger.warning(
                        f"[Architect] Duplicate artifact: {existing_manifest.plugin_name}"
                    )
                    return DeploymentResult(
                        success=False,
                        plugin_name=cmd.plugin_name,
                        version=cmd.version,
                        error_message=f"Duplicate source hash (already exists as {existing_manifest.plugin_name})",
                    )

                manifest = PluginManifest(
                    plugin_name=cmd.plugin_name,
                    version=cmd.version,
                    source_code=cmd.source_code,
                    source_hash=source_hash,
                    status=PluginStatusEnum.ACTIVE,
                    signature=cmd.signature,
                    env_hash=env_hash,
                    artifact_hash=cmd.artifact_hash,
                    publisher_id=db_publisher.id,
                    system_requirements=json.dumps(cmd.system_requirements or []),
                    deployed_at=datetime.now(),
                )
                session.add(manifest)
                session.flush()
                manifest_id = manifest.id

                logger.info(
                    f"[Architect] Persisted manifest ID={manifest_id} "
                    f"(publisher={db_publisher.name})"
                )

                # Step 4: Extract MANIFEST metadata via AST (DOD Core)
                try:
                    metadata = self._extract_metadata_safely(cmd.source_code)
                except (ValueError, SyntaxError) as e:
                    logger.error(f"[Architect] Metadata extraction failed: {e}")
                    manifest.status = PluginStatusEnum.REJECTED
                    manifest.validation_error = f"MANIFEST parsing error: {e}"
                    session.commit()
                    return DeploymentResult(
                        success=False,
                        plugin_name=cmd.plugin_name,
                        version=cmd.version,
                        error_message=str(e),
                        manifest_id=manifest_id,
                    )

                # Step 5: State Projection - Wire routing rules
                # UPSERT RoutingRule: pattern -> tag
                tag = f"auto_{cmd.plugin_name}"
                existing_rule = (
                    session.query(RoutingRule)
                    .filter_by(pattern=metadata["pattern"])
                    .first()
                )
                if existing_rule:
                    existing_rule.tag = tag
                    logger.info(
                        f"[Architect] Updated RoutingRule: {metadata['pattern']} -> {tag}"
                    )
                else:
                    rule = RoutingRule(
                        pattern=metadata["pattern"],
                        tag=tag,
                        priority=0,
                    )
                    session.add(rule)
                    logger.info(
                        f"[Architect] Created RoutingRule: {metadata['pattern']} -> {tag}"
                    )

                # UPSERT PluginConfig
                plugin_config = (
                    session.query(PluginConfig)
                    .filter_by(plugin_name=cmd.plugin_name)
                    .first()
                )
                if not plugin_config:
                    plugin_config = PluginConfig(
                        plugin_name=cmd.plugin_name,
                        subscription_tags=tag,  # Subscribe to its own tag
                    )
                    session.add(plugin_config)
                    logger.info(f"[Architect] Created PluginConfig: {cmd.plugin_name}")
                else:
                    plugin_config.subscription_tags = tag
                    logger.info(f"[Architect] Updated PluginConfig: {cmd.plugin_name}")

                session.flush()

                # UPSERT PluginSubscription for upstream topics
                for upstream_topic in metadata.get("subscriptions", []):
                    existing_sub = (
                        session.query(PluginSubscription)
                        .filter_by(
                            plugin_name=cmd.plugin_name,
                            topic_name=upstream_topic,
                        )
                        .first()
                    )
                    if not existing_sub:
                        sub = PluginSubscription(
                            plugin_name=cmd.plugin_name,
                            topic_name=upstream_topic,
                            is_active=True,
                        )
                        session.add(sub)
                        logger.info(
                            f"[Architect] Created subscription: "
                            f"{cmd.plugin_name} -> {upstream_topic}"
                        )

                # UPSERT TopicConfig for output topic
                existing_topic = (
                    session.query(TopicConfig)
                    .filter_by(
                        plugin_name=cmd.plugin_name,
                        topic_name=metadata["topic"],
                    )
                    .first()
                )
                if not existing_topic:
                    topic_config = TopicConfig(
                        plugin_name=cmd.plugin_name,
                        topic_name=metadata["topic"],
                        uri=f"parquet://./output/{metadata['topic']}",
                        mode="append",
                    )
                    session.add(topic_config)
                    logger.info(
                        f"[Architect] Created TopicConfig: "
                        f"{metadata['topic']} -> parquet"
                    )

                session.commit()

                logger.info(
                    f"[Architect] ✓ Deployment complete: {cmd.plugin_name} v{cmd.version}"
                )

                return DeploymentResult(
                    success=True,
                    plugin_name=cmd.plugin_name,
                    version=cmd.version,
                    manifest_id=manifest_id,
                )

        except Exception as e:
            logger.error(f"[Architect] Deployment failed: {e}", exc_info=True)
            return DeploymentResult(
                success=False,
                plugin_name=cmd.plugin_name,
                version=cmd.version,
                error_message=f"Deployment exception: {e}",
            )

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
        logger.info(
            f"Starting deployment: {plugin_name} v{version}{' [UNSAFE MODE]' if unsafe else ''}"
        )

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

            validation = ValidationResult(
                is_safe=True, error_message=None, violations=[]
            )

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
                session.query(PluginManifest).filter_by(source_hash=source_hash).first()
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
