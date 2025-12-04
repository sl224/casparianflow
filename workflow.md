# Casparian Flow Workflow

This document outlines how to use the Casparian Flow system for file processing and data pipeline management.

## Overview

Casparian Flow is a file monitoring and processing system with three main components:
- **Scout**: Monitors source directories and queues files for processing
- **Worker**: Processes queued files using configured plugins
- **Plugins**: Execute custom logic on files and output to configured sinks

### File Versioning

The system implements **immutable file versioning** to handle "slowly changing files" and maintain proper lineage tracking:

- **FileLocation**: Represents the persistent path/container (e.g., `data/finance.csv`)
- **FileVersion**: Immutable snapshot of file content at a point in time
- **Processing Jobs**: Link to specific versions, not mutable locations

When a file is modified, Scout creates a new `FileVersion` record instead of updating the existing one. This ensures:
- **Audit Trail**: You can always determine which version of a file was processed by each job
- **Data Integrity**: Job records permanently reference the exact content they processed
- **Lineage Tracking**: No gaps when users edit files in place (typo corrections, data updates, etc.)

## Setup

### 1. Configuration

Create a `global_config.toml` file in the project root:

```toml
[database]
db_location = "casparian_flow.sqlite3"
```

### 2. Database Initialization

The database is automatically initialized when you run the smoke test or worker for the first time.

## Running the System

### Quick Start: Smoke Test

To verify the entire system end-to-end:

```bash
uv run python scripts/smoke_test.py
```

This will:
1. Create a test directory with sample data
2. Initialize the database
3. Configure a source root and plugin
4. Run the Scout to queue files
5. Verify jobs were created

### Running the Scout

The Scout scans source directories and queues files for processing:

```python
from casparian_flow.services.scout import Scout
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.models import SourceRoot

# Get database session
db = SessionLocal()

# Get source root (assumes it exists in DB)
root = db.query(SourceRoot).first()

# Run scout
scout = Scout(db)
scout.scan_source(root)
```

### Running the Worker

The Worker processes queued jobs:

```bash
uv run -m casparian_flow.main
```

The worker will:
1. Load all plugins from `src/casparian_flow/plugins/`
2. Poll the job queue for pending work
3. Execute plugins on queued files
4. Write outputs to configured sinks (Parquet, MSSQL, etc.)

## Creating a Plugin

Plugins must inherit from `BasePlugin` and implement the `execute` method:

```python
from casparian_flow.sdk import BasePlugin
from typing import Dict, Any
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Your custom processing logic
        df = pd.read_csv(file_path)
        
        # Publish results to configured sink
        self.publish('topic_name', df)
```

**Important**: The plugin class must be named `Handler` for the loader to find it.

## Plugin Configuration

Configure plugins in the database via `PluginConfig`:

```python
from casparian_flow.db.models import PluginConfig

plugin_config = PluginConfig(
    plugin_name="my_plugin",
    topic_config='{"output_topic": {"uri": "parquet://./output", "mode": "append"}}'
)
db.add(plugin_config)
db.commit()
```

### Sink URIs

- **Parquet**: `parquet://path/to/output` (creates parquet files)
- **MSSQL**: `mssql://table_name` (writes to SQL Server)

## File Processing Flow

1. **Scout** scans source directory and finds `example.csv`
2. Scout creates `FileMetadata` record and computes hash
3. Scout creates `ProcessingJob` with appropriate plugin
4. **Worker** polls queue and gets the job
5. Worker loads the plugin and executes it with the file path
6. Plugin processes the file and calls `publish()` with results
7. Worker writes results to configured sink (Parquet/MSSQL)
8. Job is marked `COMPLETED`

## Verifying Output

After running the worker, check output files:

```bash
# Check parquet output
ls -la data/parquet/

# Read parquet file
uv run python -c "import pandas as pd; print(pd.read_parquet('data/parquet/output'))"

# Check job status in database
uv run python -c "from casparian_flow.db.base_session import SessionLocal; from casparian_flow.db.models import ProcessingJob; from sqlalchemy import create_engine; engine = create_engine('sqlite:///casparian_flow.sqlite3'); db = SessionLocal(bind=engine); jobs = db.query(ProcessingJob).all(); [print(f'Job {j.id}: Status={j.status}') for j in jobs]"
```

## Querying Version History

The versioning architecture enables powerful lineage and audit queries:

### View All Versions of a File

```python
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.models import FileLocation, FileVersion
from sqlalchemy import create_engine

engine = create_engine('sqlite:///casparian_flow.sqlite3')
db = SessionLocal(bind=engine)

# Get all versions for a specific file
location = db.query(FileLocation).filter_by(rel_path='data/finance.csv').first()
versions = db.query(FileVersion).filter_by(location_id=location.id).order_by(FileVersion.detected_at).all()

for v in versions:
    print(f"Version {v.id}: Hash={v.content_hash[:8]}..., Detected={v.detected_at}, Size={v.size_bytes}")
```

### Audit: Which Version Did a Job Process?

```python
from casparian_flow.db.models import ProcessingJob, FileVersion, FileLocation

job = db.query(ProcessingJob).get(job_id)
version = db.query(FileVersion).get(job.file_version_id)
location = db.query(FileLocation).get(version.location_id)

print(f"Job {job.id} processed {location.rel_path}")
print(f"  Version: {version.id}")
print(f"  Hash: {version.content_hash}")
print(f"  Detected: {version.detected_at}")
print(f"  Job Status: {job.status}")
```

### SQL Query: Version Timeline

```sql
-- View version history for all files
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

### SQL Query: Job-to-Version Lineage

```sql
-- Complete lineage: Job → Version → Location
SELECT 
    j.id as job_id,
    j.status,
    j.plugin_name,
    v.id as version_id,
    v.content_hash,
    l.rel_path,
    v.detected_at
FROM cf_processing_queue j
JOIN cf_file_version v ON v.id = j.file_version_id
JOIN cf_file_location l ON l.id = v.location_id
ORDER BY j.id;
```

## Troubleshooting

### Common Issues

1. **No jobs queued**: Ensure source root is configured in the database
2. **Plugin not found**: Verify plugin class is named `Handler`
3. **No output files**: Check `PluginConfig.topic_config` for correct sink URI
4. **Worker exits immediately**: Ensure there are jobs in `QUEUED` status

### Logs

Enable debug logging:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
```
