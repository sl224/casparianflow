# src/casparian_flow/mcp_server.py
import json
import logging
from mcp.server.fastmcp import FastMCP
from sqlalchemy.orm import Session
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.models import (
    FileLocation,
    FileVersion,
    ProcessingJob,
    StatusEnum,
    PluginConfig,
    RoutingRule,
    SourceRoot,
    LibraryWhitelist,
    PluginManifest,
    SurveyorSession,
    PhaseEnum,
)
from casparian_flow.services.scout import Scout
from casparian_flow.services.inspector import profile_file
from casparian_flow.services.architect import ArchitectService
from casparian_flow.security.gatekeeper import generate_signature
from pathlib import Path

# Initialize the Server
mcp = FastMCP("Casparian Flow")

logger = logging.getLogger(__name__)


def get_db():
    """Helper to get a DB session."""
    return SessionLocal()


def get_surveyor_agent(db: Session) -> 'SurveyorAgent':
    """
    Factory for SurveyorAgent with all dependencies.

    Returns a fully configured SurveyorAgent instance.
    """
    from casparian_flow.agents.surveyor import SurveyorAgent
    from casparian_flow.services.llm_provider import get_provider
    from casparian_flow.services.llm_generator import LLMGenerator
    from casparian_flow.services.test_generator import TestGenerator

    # Configuration (TODO: load from environment variables)
    secret_key = "mcp-surveyor-key"
    llm_provider_type = "anthropic"  # or "openai", "claude-cli", "manual"

    # Initialize services
    try:
        llm_provider = get_provider(llm_provider_type)
    except Exception as e:
        # Fallback to manual provider if configured provider fails
        logger.warning(f"Failed to initialize {llm_provider_type} provider: {e}, falling back to manual")
        llm_provider = get_provider("manual")

    llm_generator = LLMGenerator(llm_provider)
    test_generator = TestGenerator(llm_generator)
    scout = Scout(db)
    architect = ArchitectService(db.get_bind(), secret_key)

    return SurveyorAgent(
        db_session=db,
        scout=scout,
        architect=architect,
        llm_generator=llm_generator,
        test_generator=test_generator,
    )


# --- RESOURCES (READ ACCESS) ---


@mcp.resource("casparian://files/recent")
def list_recent_files() -> str:
    """Returns a list of the 10 most recently discovered files."""
    with get_db() as db:
        files = (
            db.query(FileLocation)
            .order_by(FileLocation.discovered_time.desc())
            .limit(10)
            .all()
        )
        return "\n".join(
            [
                f"ID: {f.id} | Path: {f.rel_path} | Tags: {f.current_version.applied_tags if f.current_version else ''}"
                for f in files
            ]
        )


@mcp.resource("casparian://logs/failures")
def list_failed_jobs() -> str:
    """Returns a summary of currently failing jobs."""
    with get_db() as db:
        jobs = (
            db.query(ProcessingJob)
            .filter(ProcessingJob.status == StatusEnum.FAILED)
            .limit(20)
            .all()
        )
        report = []
        for job in jobs:
            report.append(
                f"Job {job.id} | Plugin: {job.plugin_name} | Error: {job.error_message[:100]}..."
            )
        return "\n".join(report)


@mcp.resource("casparian://file/{file_id}/content")
def read_file_content(file_id: int) -> str:
    """
    Reads the raw content of a file (first 2KB) for inspection.
    This enables the 'Analyze File' workflow.
    """
    with get_db() as db:
        loc = db.query(FileLocation).get(file_id)
        if not loc:
            return "File not found."

        # Security: Ensure we resolve this relative to the SourceRoot to prevent traversal
        # (Simplified logic here - assume SourceRoot path is available via relationship)
        full_path = Path(loc.source_root.path) / loc.rel_path

        try:
            # Only read the head to avoid blowing up the context window
            with open(full_path, "r", encoding="utf-8", errors="replace") as f:
                return f.read(2048)
        except Exception as e:
            return f"Error reading file: {str(e)}"


# --- TOOLS (WRITE/ACTION ACCESS) ---


@mcp.tool()
def trigger_network_scan(source_root_id: int):
    """
    Forces the Scout to immediately scan a specific Source Root ID.
    Use this after adding new files to the network drive.
    """
    with get_db() as db:
        from casparian_flow.db.models import SourceRoot

        root = db.query(SourceRoot).get(source_root_id)
        if not root:
            return f"Source Root {source_root_id} not found."

        scout = Scout(db)
        scout.scan_source(root)
        return f"Scan complete for {root.path}."


@mcp.tool()
def add_routing_rule(pattern: str, tag: str, priority: int = 10):
    """
    Adds a new Metadata Routing Rule to the system.
    Example: pattern='finance/*.csv', tag='finance_data'
    """
    with get_db() as db:
        rule = RoutingRule(pattern=pattern, tag=tag, priority=priority)
        db.add(rule)
        db.commit()
        return f"Rule added: '{pattern}' -> '{tag}'"


@mcp.tool()
def replay_failed_jobs(plugin_name: str):
    """
    Resets all FAILED jobs for a specific plugin to QUEUED.
    Use this after fixing a plugin code issue.
    """
    with get_db() as db:
        count = (
            db.query(ProcessingJob)
            .filter(
                ProcessingJob.status == StatusEnum.FAILED,
                ProcessingJob.plugin_name == plugin_name,
            )
            .update({ProcessingJob.status: StatusEnum.QUEUED})
        )
        db.commit()
        return f"Requeued {count} jobs for {plugin_name}."


# --- SURVEYOR AGENT TOOLS ---


@mcp.tool()
def register_and_scan_path(path: str):
    """
    Attach system to a folder and index all files.

    Args:
        path: Absolute path to directory

    Returns:
        Summary of scan results (files discovered, new files, etc.)
    """
    with get_db() as db:
        # Get or create SourceRoot
        root = db.query(SourceRoot).filter_by(path=path).first()
        if not root:
            root = SourceRoot(path=path, type="local", active=1)
            db.add(root)
            db.commit()
            db.refresh(root)

        # Trigger scan
        scout = Scout(db)
        scout.scan_source(root)

        # Get statistics
        file_count = db.query(FileLocation).filter_by(source_root_id=root.id).count()
        version_count = db.query(FileVersion).join(FileLocation).filter(
            FileLocation.source_root_id == root.id
        ).count()

        return f"Scan complete for '{path}'. Files: {file_count}, Versions: {version_count}"


@mcp.tool()
def sample_unprocessed_files(limit: int = 5):
    """
    Get files that have no active plugin configured.

    Args:
        limit: Maximum number of samples to return

    Returns:
        JSON list of file metadata (id, path, size, tags)
    """
    with get_db() as db:
        # Query FileVersions with no ProcessingJob entries
        # This indicates no plugin has claimed them
        unprocessed_query = (
            db.query(FileVersion)
            .outerjoin(ProcessingJob, ProcessingJob.file_version_id == FileVersion.id)
            .filter(ProcessingJob.id == None)  # No jobs exist
            .limit(limit)
        )

        unprocessed = unprocessed_query.all()

        # Also include files with empty tags
        empty_tags_query = (
            db.query(FileVersion)
            .filter(FileVersion.applied_tags == "")
            .limit(limit)
        )

        empty_tags = empty_tags_query.all()

        # Combine and deduplicate
        all_unprocessed = list(set(unprocessed + empty_tags))[:limit]

        results = []
        for fv in all_unprocessed:
            loc = db.query(FileLocation).get(fv.location_id)
            if loc:
                results.append({
                    "file_version_id": fv.id,
                    "file_location_id": loc.id,
                    "path": loc.rel_path,
                    "filename": loc.filename,
                    "size_bytes": fv.size_bytes,
                    "tags": fv.applied_tags,
                    "content_hash": fv.content_hash,
                })

        return json.dumps(results, indent=2)


@mcp.tool()
def inspect_file_header(file_id: int, bytes_to_read: int = 512):
    """
    Read HEX signature to determine true file type.

    Args:
        file_id: FileLocation ID
        bytes_to_read: Number of bytes to read (default 512)

    Returns:
        JSON with file profile including hex dump and detected type
    """
    with get_db() as db:
        # Get FileLocation
        loc = db.query(FileLocation).get(file_id)
        if not loc:
            return json.dumps({"error": f"FileLocation {file_id} not found"})

        # Resolve full path
        full_path = Path(loc.source_root.path) / loc.rel_path

        if not full_path.exists():
            return json.dumps({"error": f"File not found: {full_path}"})

        try:
            # Use inspector to profile file
            from casparian_flow.services.ai_types import FileType

            profile = profile_file(str(full_path))

            # Read raw bytes for hex dump
            with open(full_path, "rb") as f:
                raw_bytes = f.read(bytes_to_read)

            hex_dump = raw_bytes.hex()

            result = {
                "file_id": file_id,
                "path": str(full_path),
                "file_type": profile.file_type.name,
                "total_size": profile.total_size,
                "hex_header": hex_dump[:256],  # First 128 bytes in hex
                "encoding": profile.head_sample.encoding_detected,
                "metadata_hints": profile.metadata_hints,
            }

            return json.dumps(result, indent=2)

        except Exception as e:
            return json.dumps({"error": str(e)})


@mcp.tool()
def list_allowed_libraries():
    """
    Check which Python packages are installed and whitelisted.

    Returns:
        JSON list of {library, version, description}
    """
    with get_db() as db:
        libs = db.query(LibraryWhitelist).all()

        result = [
            {
                "library": lib.library_name,
                "version": lib.version_constraint,
                "description": lib.description,
            }
            for lib in libs
        ]

        return json.dumps(result, indent=2)


@mcp.tool()
def deploy_plugin(name: str, code: str):
    """
    Write and validate plugin, returns errors if any.

    Args:
        name: Plugin name
        code: Python source code

    Returns:
        DeploymentResult as JSON
    """
    with get_db() as db:
        # Get secret key from config (TODO: load from env)
        secret_key = "mcp-surveyor-key"

        architect = ArchitectService(db.get_bind(), secret_key)

        # Generate signature
        signature = generate_signature(code, secret_key)

        # Deploy with sample input skipped (no sandbox for now)
        result = architect.deploy_plugin(
            plugin_name=name,
            version="1.0.0",
            source_code=code,
            signature=signature,
            sample_input=None,
        )

        response = {
            "success": result.success,
            "plugin_name": name,
            "manifest_id": result.manifest_id,
            "error": result.error_message,
            "validation_errors": result.validation_errors if hasattr(result, 'validation_errors') else [],
        }

        return json.dumps(response, indent=2)


@mcp.tool()
def configure_routing(pattern: str, tag: str, plugin_name: str):
    """
    Map file patterns to plugins via tags.

    Args:
        pattern: Glob pattern (e.g., "*.csv")
        tag: Tag to apply (e.g., "csv_data")
        plugin_name: Plugin to subscribe to tag

    Returns:
        Confirmation message
    """
    with get_db() as db:
        # Create routing rule
        rule = RoutingRule(pattern=pattern, tag=tag, priority=10)
        db.add(rule)

        # Update or create PluginConfig
        config = db.query(PluginConfig).filter_by(plugin_name=plugin_name).first()
        if not config:
            config = PluginConfig(plugin_name=plugin_name, subscription_tags=tag)
            db.add(config)
        else:
            # Append tag if not already present
            existing_tags = set(config.subscription_tags.split(",")) if config.subscription_tags else set()
            existing_tags.add(tag)
            config.subscription_tags = ",".join(sorted(existing_tags))

        db.commit()

        return f"Routing configured: {pattern} -> {tag} -> {plugin_name}"


@mcp.tool()
def get_system_status():
    """
    Check job queue depth and recent failures.

    Returns:
        JSON with queue stats and error summary
    """
    with get_db() as db:
        queued = db.query(ProcessingJob).filter_by(status=StatusEnum.QUEUED).count()
        running = db.query(ProcessingJob).filter_by(status=StatusEnum.RUNNING).count()
        completed = db.query(ProcessingJob).filter_by(status=StatusEnum.COMPLETED).count()
        failed = db.query(ProcessingJob).filter_by(status=StatusEnum.FAILED).limit(10).all()

        recent_failures = [
            {
                "id": j.id,
                "plugin": j.plugin_name,
                "file_version_id": j.file_version_id,
                "error": j.error_message[:200] if j.error_message else "No error message",
                "retry_count": j.retry_count,
            }
            for j in failed
        ]

        result = {
            "queued_jobs": queued,
            "running_jobs": running,
            "completed_jobs": completed,
            "failed_jobs_count": len(failed),
            "recent_failures": recent_failures,
        }

        return json.dumps(result, indent=2)


# --- SURVEYOR RESOURCE ---


@mcp.resource("casparian://surveyor/sessions")
def list_surveyor_sessions() -> str:
    """List recent surveyor sessions."""
    with get_db() as db:
        sessions = (
            db.query(SurveyorSession)
            .order_by(SurveyorSession.started_at.desc())
            .limit(10)
            .all()
        )

        if not sessions:
            return "No surveyor sessions found."

        lines = []
        for s in sessions:
            status = s.current_phase.value if s.current_phase else "UNKNOWN"
            lines.append(
                f"Session {s.id} | Phase: {status} | Root: {s.source_root_id} | Started: {s.started_at}"
            )

        return "\n".join(lines)


if __name__ == "__main__":
    mcp.run()
