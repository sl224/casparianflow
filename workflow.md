# Casparian Flow Workflow

This document outlines how to use the Casparian Flow system for file processing and data pipeline management.

## Overview

Casparian Flow is a file monitoring and processing system with these main components:
- **Scout**: Monitors source directories and queues files for processing
- **Sentinel**: Control plane broker that orchestrates workers
- **Worker**: Processes queued files using configured plugins
- **Plugins**: Execute custom logic on files and output to configured sinks
- **Architect**: Manages plugin deployment and auto-wiring

### File Versioning

The system implements **immutable file versioning** to handle "slowly changing files" and maintain proper lineage tracking:

- **FileLocation**: Represents the persistent path/container (e.g., `data/finance.csv`)
- **FileVersion**: Immutable snapshot of file content at a point in time
- **ProcessingJob**: Links to specific versions, not mutable locations

When a file is modified, Scout creates a new `FileVersion` record instead of updating the existing one. This ensures:
- **Audit Trail**: You can always determine which version of a file was processed by each job
- **Data Integrity**: Job records permanently reference the exact content they processed
- **Lineage Tracking**: No gaps when users edit files in place

## Setup

### 1. Configuration

Create a `global_config.toml` file in the project root:

```toml
[database]
db_location = "casparian_flow.sqlite3"
```

### 2. Database Initialization

The database is automatically initialized when you run the system for the first time.

## Running the System

### Quick Start (Rust Binary)

```bash
# Build the unified binary
cargo build --release

# Run both Sentinel and Worker in a single process
./target/release/casparian start
```

**Legacy Python mode** (deprecated):
```bash
uv run -m casparian_flow.main  # Sentinel only
uv run -m casparian_flow.engine.worker_client --connect tcp://localhost:5555  # Worker
```

### Publishing a Plugin (Rust Implementation)

```bash
# Publish a plugin with automatic signing
./target/release/casparian publish my_plugin.py --version 1.0.0
```

This will:
1. Lock dependencies with `uv lock --universal`
2. Compute artifact hash (SHA-256)
3. Authenticate (Local keys or Azure AD Device Code Flow)
4. Sign with Ed25519 (`cf_security::signing`)
5. Validate via Gatekeeper (AST analysis)
6. Deploy to Sentinel

**Enterprise Mode** (Azure AD):
```bash
export AZURE_TENANT_ID="your-tenant-id"
export AZURE_CLIENT_ID="your-client-id"
./target/release/casparian publish my_plugin.py --version 1.0.0
```
Follow the device code flow prompts in the terminal.

### Running the Scout

The Scout scans source directories and queues files for processing:

```python
from casparian_flow.services.scout import Scout
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import SourceRoot
from casparian_flow.config import settings
from sqlalchemy.orm import Session

engine = get_engine(settings.database)
db = Session(engine)

# Get source root
root = db.query(SourceRoot).first()

# Run scout
scout = Scout(db)
scout.scan_source(root)
```

## Creating a Plugin

Plugins must inherit from `BasePlugin` and implement the `execute` method:

```python
from casparian_flow.sdk import BasePlugin, PluginMetadata
import pandas as pd

MANIFEST = PluginMetadata(
    pattern="*.csv",           # Auto-creates RoutingRule
    topic="sales_data",        # Auto-creates TopicConfig
    priority=50,
    subscriptions=["csv"]
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        df = pd.read_csv(file_path)

        # Publish results to configured sink
        self.publish('sales_data', df)
```

**Important**: The plugin class must be named `Handler` for the loader to find it.

## Sink URIs

- **Parquet**: `parquet://path/to/output` (creates parquet files)
- **SQLite**: `sqlite:///path/to/db.sqlite3` (writes to SQLite)
- **MSSQL**: `mssql://table_name` (writes to SQL Server)

## File Processing Flow

1. **Scout** scans source directory and finds `example.csv`
2. Scout creates `FileLocation` record and computes hash
3. Scout creates `FileVersion` with content hash
4. Scout applies routing rules and creates `ProcessingJob`
5. **Worker** polls Sentinel and receives job dispatch
6. Worker loads the plugin and executes it with the file path
7. Plugin processes the file and calls `publish()` with results
8. Worker writes results to configured sink (Parquet/SQL)
9. Worker sends `CONCLUDE` receipt to Sentinel
10. Job is marked `COMPLETED`

## Querying Version History

### View All Versions of a File

```python
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import FileLocation, FileVersion
from casparian_flow.config import settings
from sqlalchemy.orm import Session

engine = get_engine(settings.database)
db = Session(engine)

# Get all versions for a specific file
location = db.query(FileLocation).filter_by(rel_path='data/finance.csv').first()
versions = db.query(FileVersion).filter_by(location_id=location.id).order_by(FileVersion.detected_at).all()

for v in versions:
    print(f"Version {v.id}: Hash={v.content_hash[:8]}..., Detected={v.detected_at}")
```

### SQL Query: Version Timeline

```sql
SELECT
    l.rel_path,
    v.id as version_id,
    v.content_hash,
    v.size_bytes,
    v.detected_at,
    COUNT(j.id) as jobs_processed
FROM cf_file_location l
JOIN cf_file_version v ON v.location_id = l.id
LEFT JOIN cf_processing_queue j ON j.file_version_id = v.id
GROUP BY l.rel_path, v.id
ORDER BY l.rel_path, v.detected_at;
```

## Troubleshooting

### Common Issues

1. **No jobs queued**: Ensure source root is configured and routing rules match
2. **Plugin not found**: Verify plugin class is named `Handler`
3. **No output files**: Check `TopicConfig` for correct sink URI
4. **Worker exits immediately**: Ensure there are jobs in `QUEUED` status

### Logs

Enable debug logging:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
```
