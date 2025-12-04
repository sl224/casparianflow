# src/casparian_flow/mcp_server.py
from mcp.server.fastmcp import FastMCP
from sqlalchemy.orm import Session
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.models import (
    FileLocation, FileVersion, ProcessingJob, StatusEnum, PluginConfig, RoutingRule
)
from casparian_flow.services.scout import Scout
from pathlib import Path

# Initialize the Server
mcp = FastMCP("Casparian Flow")

def get_db():
    """Helper to get a DB session."""
    return SessionLocal()

# --- RESOURCES (READ ACCESS) ---

@mcp.resource("casparian://files/recent")
def list_recent_files() -> str:
    """Returns a list of the 10 most recently discovered files."""
    with get_db() as db:
        files = db.query(FileLocation).order_by(FileLocation.discovered_time.desc()).limit(10).all()
        return "\n".join([f"ID: {f.id} | Path: {f.rel_path} | Tags: {f.current_version.applied_tags if f.current_version else ''}" for f in files])

@mcp.resource("casparian://logs/failures")
def list_failed_jobs() -> str:
    """Returns a summary of currently failing jobs."""
    with get_db() as db:
        jobs = db.query(ProcessingJob).filter(ProcessingJob.status == StatusEnum.FAILED).limit(20).all()
        report = []
        for job in jobs:
            report.append(f"Job {job.id} | Plugin: {job.plugin_name} | Error: {job.error_message[:100]}...")
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
        count = db.query(ProcessingJob).filter(
            ProcessingJob.status == StatusEnum.FAILED,
            ProcessingJob.plugin_name == plugin_name
        ).update({ProcessingJob.status: StatusEnum.QUEUED})
        db.commit()
        return f"Requeued {count} jobs for {plugin_name}."

if __name__ == "__main__":
    mcp.run()