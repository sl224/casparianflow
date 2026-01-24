# Export - Feature Specification

**Status:** Draft
**Version:** 1.0
**Last Updated:** January 2026

---

## 1. Overview

**Export** transforms parsed output (Parquet/SQLite) into domain-specific formats required by downstream tools. This is not "save as CSV" - it's semantic transformation that understands the target format's requirements.

### 1.1 Why Export Matters

After parsing, users need to move data into vertical-specific tools:

| Vertical | Downstream Tool | Required Format |
|----------|-----------------|-----------------|
| **Legal** | Relativity, Concordance, Nuix | Load files with control numbers, family relationships, hash chains |
| **Finance** | Bloomberg TCA, compliance systems | Venue-normalized trades, regulatory timestamps |
| **Healthcare** | Epic, Cerner, BI tools | FHIR bundles, CCD documents |
| **Defense** | Google Earth, ArcGIS, briefing tools | KML, GeoJSON, slide-ready data |

Each format has complex requirements:
- **Concordance DAT**: Specific delimiters (þ), encoding (UTF-8 with BOM), control number sequences
- **FHIR Bundle**: Resource references, identifier systems, code mappings
- **KML**: Coordinate systems, time spans, style inheritance

### 1.2 Core Insight

**Exporters are the dual of Parsers:**
- Parser: Raw files → Structured data (Parquet)
- Exporter: Structured data → Downstream format

Both are Python classes with version/schema contracts. Both run as jobs.

### 1.3 Design Principles

1. **Exporters are code** - Not configuration. Complex formats need logic.
2. **Versioned and auditable** - Same governance as parsers.
3. **Incremental when possible** - Don't re-export unchanged data.
4. **Fail loudly** - Format violations are hard errors.

---

## 2. User Workflows

### 2.1 Legal: Export PST to Concordance Load File

```
1. User has parsed PST archive into parquet:
   ~/.casparian_flow/output/emails/emails_job123.parquet

2. User runs export:
   $ casparian export concordance \
       --input ~/.casparian_flow/output/emails/*.parquet \
       --output ./production_001/ \
       --matter "Smith v. Jones" \
       --bates-prefix "SMITH" \
       --bates-start 000001

3. Export job runs:
   - Reads parquet files
   - Generates control numbers (SMITH000001, SMITH000002, ...)
   - Computes MD5/SHA1 hashes for deduplication
   - Builds family relationships (email → attachments)
   - Writes DAT file with þ delimiters
   - Writes OPT file (image references)
   - Copies native files with Bates names

4. Output:
   ./production_001/
   ├── loadfile.dat          # Concordance DAT
   ├── loadfile.opt          # Image pointers
   ├── NATIVES/
   │   ├── SMITH000001.msg
   │   ├── SMITH000002.pdf
   └── manifest.json         # Export metadata
```

### 2.2 Finance: Export FIX to TCA Format

```
1. User has parsed FIX logs:
   ~/.casparian_flow/output/fix_orders/*.parquet

2. User runs export:
   $ casparian export bloomberg-tca \
       --input ~/.casparian_flow/output/fix_orders/*.parquet \
       --output ./tca_upload.csv \
       --venue-map ./venue_mappings.json \
       --date-range 2024-01-15

3. Export job runs:
   - Reads order lifecycle data
   - Maps internal venue codes to Bloomberg MIC codes
   - Normalizes timestamps to UTC
   - Calculates fill rates, slippage metrics
   - Formats per Bloomberg TCA spec

4. Output:
   ./tca_upload.csv           # Ready for Bloomberg upload
   ./tca_export_manifest.json # Audit trail
```

### 2.3 Healthcare: Export HL7 to FHIR Bundle

```
1. User has parsed HL7 archive:
   ~/.casparian_flow/output/hl7_messages/*.parquet

2. User runs export:
   $ casparian export fhir-r4 \
       --input ~/.casparian_flow/output/hl7_messages/*.parquet \
       --output ./fhir_bundles/ \
       --bundle-type transaction \
       --identifier-system "urn:oid:2.16.840.1.113883.3.123"

3. Export job runs:
   - Maps HL7 segments to FHIR resources
   - Creates Patient, Encounter, Observation resources
   - Links resources via references
   - Validates against FHIR R4 schema

4. Output:
   ./fhir_bundles/
   ├── patient_001.json       # FHIR Bundle
   ├── patient_002.json
   └── export_manifest.json
```

### 2.4 Defense: Export CoT to KML

```
1. User has parsed CoT tracks:
   ~/.casparian_flow/output/cot_tracks/*.parquet

2. User runs export:
   $ casparian export kml \
       --input ~/.casparian_flow/output/cot_tracks/*.parquet \
       --output ./mission_tracks.kml \
       --time-window "2024-01-15T00:00:00Z/2024-01-15T23:59:59Z" \
       --color-by affiliation

3. Export job runs:
   - Filters to time window
   - Converts coordinates to WGS84
   - Groups by track UID
   - Applies styling by affiliation (blue/red/neutral)
   - Generates KML with TimeSpan elements

4. Output:
   ./mission_tracks.kml       # Google Earth ready
   ./mission_tracks.kmz       # Compressed with icons
```

---

## 3. CLI Interface

### 3.1 Basic Usage

```bash
# List available exporters
casparian export --list

# Run an export
casparian export <exporter> --input <files> --output <destination> [options]

# Dry run (validate without writing)
casparian export <exporter> --input <files> --output <dest> --dry-run

# Show exporter help
casparian export <exporter> --help
```

### 3.2 Common Options

| Flag | Description | Example |
|------|-------------|---------|
| `--input` | Input parquet/duckdb files (glob) | `./output/*.parquet` |
| `--output` | Output file or directory | `./export/` |
| `--dry-run` | Validate without writing | |
| `--force` | Overwrite existing output | |
| `--incremental` | Only export new/changed records | |
| `--job-id` | Track as specific job | `--job-id abc123` |

### 3.3 Exporter-Specific Options

Each exporter defines its own options:

```bash
# Concordance
casparian export concordance \
    --bates-prefix SMITH \
    --bates-start 000001 \
    --date-format "MM/DD/YYYY" \
    --timezone "America/New_York"

# FHIR
casparian export fhir-r4 \
    --bundle-type transaction \
    --identifier-system "urn:oid:..." \
    --profile "http://hl7.org/fhir/us/core"

# KML
casparian export kml \
    --altitude-mode clampToGround \
    --color-by affiliation \
    --icon-scale 1.0
```

---

## 4. Exporter Class Specification

### 4.1 Exporter Interface

Exporters are Python classes similar to parsers:

```python
import pyarrow as pa
import pyarrow.parquet as pq
from pathlib import Path
from typing import Iterator, Any

class ConcordanceExporter:
    """Export parsed emails to Concordance DAT format."""

    # Required metadata
    name = "concordance"
    version = "1.0.0"

    # Input schema this exporter expects
    input_schema = pa.schema([
        ("message_id", pa.string()),
        ("from_addr", pa.string()),
        ("to_addrs", pa.list_(pa.string())),
        ("subject", pa.string()),
        ("body", pa.string()),
        ("sent_date", pa.timestamp("us")),
        ("attachments", pa.list_(pa.struct([
            ("filename", pa.string()),
            ("content_hash", pa.string()),
            ("path", pa.string()),
        ]))),
    ])

    # Configuration options
    class Config:
        bates_prefix: str = "DOC"
        bates_start: int = 1
        date_format: str = "MM/DD/YYYY"
        timezone: str = "UTC"
        delimiter: str = "\xfe"  # þ character
        quote: str = "\x14"      # Concordance quote
        newline: str = "\xae"    # ® for newlines in fields

    def __init__(self, config: Config):
        self.config = config
        self.bates_counter = config.bates_start

    def export(self, input_files: list[Path], output_dir: Path) -> ExportResult:
        """
        Main export method.

        Args:
            input_files: List of parquet files to export
            output_dir: Directory to write output files

        Returns:
            ExportResult with statistics and manifest
        """
        output_dir.mkdir(parents=True, exist_ok=True)

        dat_path = output_dir / "loadfile.dat"
        opt_path = output_dir / "loadfile.opt"
        natives_dir = output_dir / "NATIVES"
        natives_dir.mkdir(exist_ok=True)

        records_exported = 0

        with open(dat_path, "w", encoding="utf-8-sig") as dat_file:
            # Write header
            dat_file.write(self._format_header())

            for input_file in input_files:
                table = pq.read_table(input_file)

                for batch in table.to_batches():
                    for row in self._process_batch(batch):
                        dat_file.write(self._format_row(row))
                        records_exported += 1

        return ExportResult(
            records_exported=records_exported,
            output_files=[dat_path, opt_path],
            manifest=self._create_manifest(output_dir),
        )

    def _next_bates(self) -> str:
        """Generate next Bates number."""
        bates = f"{self.config.bates_prefix}{self.bates_counter:06d}"
        self.bates_counter += 1
        return bates

    def _format_row(self, row: dict) -> str:
        """Format a single row for DAT file."""
        d = self.config.delimiter
        q = self.config.quote

        # Escape newlines in text fields
        subject = row["subject"].replace("\n", self.config.newline)

        return f"{q}{row['bates']}{q}{d}{q}{row['from']}{q}{d}...\n"
```

### 4.2 ExportResult

```python
@dataclass
class ExportResult:
    """Result of an export operation."""
    records_exported: int
    output_files: list[Path]
    manifest: dict
    warnings: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
```

### 4.3 Export Context

Exporters receive context similar to parsers:

```python
@dataclass
class ExportContext:
    """Context passed to exporter."""
    job_id: str
    input_files: list[Path]
    output_dir: Path
    config: dict[str, Any]

    # For incremental exports
    last_export_time: Optional[datetime] = None
    last_export_job_id: Optional[str] = None
```

---

## 5. Built-in Exporters

### 5.1 Legal

| Exporter | Target | Key Features |
|----------|--------|--------------|
| `concordance` | Concordance Desktop/Viewer | DAT + OPT, Bates numbering, family relationships |
| `relativity` | Relativity (kCura) | RDC format, overlay support, extracted text |
| `nuix` | Nuix Workstation | Nuix load file, evidence container |
| `edrm-xml` | EDRM standard | XML manifest, portable across platforms |
| `csv-legal` | Generic CSV | All metadata fields, legal-specific columns |

### 5.2 Finance

| Exporter | Target | Key Features |
|----------|--------|--------------|
| `bloomberg-tca` | Bloomberg TCA | Venue normalization, fill metrics |
| `excel-trades` | Excel pivot tables | Pre-built analysis templates |
| `fix-replay` | FIX simulators | Reconstructed message stream |
| `csv-finance` | Generic CSV | All fields, UTC timestamps |

### 5.3 Healthcare

| Exporter | Target | Key Features |
|----------|--------|--------------|
| `fhir-r4` | FHIR R4 API | Bundles, resource references |
| `fhir-stu3` | FHIR STU3 (legacy) | Older FHIR format |
| `ccd` | C-CDA documents | Clinical summaries |
| `csv-healthcare` | BI tools | Flattened patient records |

### 5.4 Defense

| Exporter | Target | Key Features |
|----------|--------|--------------|
| `kml` | Google Earth | Tracks, placemarks, time animation |
| `geojson` | Web maps, GIS | Standard GeoJSON |
| `shapefile` | ArcGIS, QGIS | ESRI shapefile format |
| `cot-replay` | TAK, ATAK | CoT XML for replay |

### 5.5 Generic

| Exporter | Target | Key Features |
|----------|--------|--------------|
| `csv` | Any CSV consumer | Configurable delimiter, encoding |
| `json` | JSON consumers | Nested or flattened |
| `jsonl` | Streaming JSON | Line-delimited JSON |
| `excel` | Excel | Multiple sheets, formatting |

---

## 6. Custom Exporters

Users can create custom exporters for proprietary formats.

### 6.1 Creating a Custom Exporter

```python
# my_exporter.py

class MyTCAExporter:
    """Export to our internal TCA format."""

    name = "my-tca"
    version = "1.0.0"

    input_schema = pa.schema([
        ("order_id", pa.string()),
        ("symbol", pa.string()),
        ("side", pa.string()),
        ("qty", pa.int64()),
        ("price", pa.float64()),
        ("timestamp", pa.timestamp("us")),
    ])

    class Config:
        include_cancelled: bool = False
        round_prices: int = 4

    def __init__(self, config: Config):
        self.config = config

    def export(self, input_files: list[Path], output_dir: Path) -> ExportResult:
        # Custom export logic
        ...
```

### 6.2 Registering Custom Exporters

```bash
# Register an exporter
casparian exporter register ./my_exporter.py

# List registered exporters (built-in + custom)
casparian export --list

# Use custom exporter
casparian export my-tca --input ./orders/*.parquet --output ./tca/
```

### 6.3 Exporter Discovery

Exporters are discovered from:
1. Built-in exporters (`casparian_worker/exporters/`)
2. User exporters (`~/.casparian_flow/exporters/`)
3. Project exporters (`./.casparian/exporters/`)

---

## 7. Job Integration

Export runs as a tracked job, just like Parse.

### 7.1 Job Record

```sql
-- Export jobs in cf_job_status
INSERT INTO cf_job_status (
    id,
    job_type,           -- 'export'
    exporter_name,      -- 'concordance'
    exporter_version,   -- '1.0.0'
    status,             -- 'running'
    input_files,        -- JSON array of input paths
    output_path,        -- '/path/to/output/'
    config,             -- JSON exporter config
    records_total,      -- NULL until known
    records_exported,   -- Progress counter
    started_at,
    completed_at,
    error_message
);
```

### 7.2 Job Status Flow

```
┌─────────┐     ┌─────────┐     ┌──────────┐
│ QUEUED  │────▶│ RUNNING │────▶│ COMPLETE │
└─────────┘     └────┬────┘     └──────────┘
                     │
                     │ error
                     ▼
                ┌─────────┐
                │ FAILED  │
                └─────────┘
```

### 7.3 Progress Reporting

For large exports, progress is reported:

```python
def export(self, input_files, output_dir) -> ExportResult:
    total_records = self._count_records(input_files)
    self.report_progress(0, total_records)

    for i, record in enumerate(self._iter_records(input_files)):
        self._write_record(record)
        if i % 1000 == 0:
            self.report_progress(i, total_records)

    self.report_progress(total_records, total_records)
```

### 7.4 Incremental Exports

For ongoing workflows, exports can be incremental:

```bash
# First export: all records
casparian export concordance --input ./emails/*.parquet --output ./prod_001/

# Later: only records added since last export
casparian export concordance --input ./emails/*.parquet --output ./prod_002/ \
    --incremental --since-job <job_id>
```

Implementation uses `_cf_processed_at` lineage column:

```sql
SELECT * FROM emails
WHERE _cf_processed_at > (
    SELECT completed_at FROM cf_job_status WHERE id = :since_job_id
);
```

---

## 8. Audit Trail & Lineage

### 8.1 Export Manifest

Every export creates a manifest file:

```json
{
    "export_id": "exp_20240115_abc123",
    "exporter": {
        "name": "concordance",
        "version": "1.0.0"
    },
    "input": {
        "files": [
            "~/.casparian_flow/output/emails/emails_job123.parquet"
        ],
        "total_records": 45782,
        "source_job_ids": ["job_abc", "job_def"]
    },
    "output": {
        "directory": "./production_001/",
        "files": [
            {"name": "loadfile.dat", "size": 12345678, "sha256": "..."},
            {"name": "loadfile.opt", "size": 234567, "sha256": "..."}
        ],
        "records_exported": 45782
    },
    "config": {
        "bates_prefix": "SMITH",
        "bates_start": 1,
        "bates_end": 45782
    },
    "timing": {
        "started_at": "2024-01-15T10:30:00Z",
        "completed_at": "2024-01-15T10:45:32Z",
        "duration_seconds": 932
    }
}
```

### 8.2 Database Tracking

```sql
-- Track exports for lineage
CREATE TABLE cf_exports (
    id TEXT PRIMARY KEY,
    exporter_name TEXT NOT NULL,
    exporter_version TEXT NOT NULL,
    job_id TEXT REFERENCES cf_job_status(id),
    input_job_ids TEXT,           -- JSON array of source parse job IDs
    output_path TEXT NOT NULL,
    config TEXT,                  -- JSON
    records_exported INTEGER,
    manifest_hash TEXT,           -- SHA256 of manifest.json
    created_at TEXT DEFAULT (datetime('now'))
);
```

### 8.3 Lineage Query

"What exports used data from this parse job?"

```sql
SELECT e.*
FROM cf_exports e
WHERE json_each.value = :parse_job_id
AND json_each.key IS NOT NULL;
-- (using JSON array containment)
```

---

## 9. Error Handling

### 9.1 Validation Errors

Exporters validate input data against expected schema:

```python
def export(self, input_files, output_dir):
    for input_file in input_files:
        table = pq.read_table(input_file)

        # Validate schema
        if not self._schema_compatible(table.schema, self.input_schema):
            raise ExportError(
                f"Input schema mismatch in {input_file}",
                expected=self.input_schema,
                actual=table.schema,
                suggestion="Ensure parser output matches exporter input schema"
            )
```

### 9.2 Format Errors

Target format validation:

```python
def _validate_bates(self, value: str) -> None:
    """Validate Bates number format."""
    if not re.match(r'^[A-Z]+\d{6,}$', value):
        raise ExportError(
            f"Invalid Bates number: {value}",
            suggestion="Bates must be PREFIX followed by 6+ digits"
        )
```

### 9.3 Recovery

Failed exports can be retried:

```bash
# Retry failed export
casparian job retry <export_job_id>

# Resume from checkpoint (if exporter supports it)
casparian export concordance --resume <export_job_id>
```

---

## 10. TUI Integration

Export jobs appear in the Jobs view alongside Parse and Scan jobs.

### 10.1 Job List Display

```
┌─ JOBS ──────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  TYPE      NAME              STATUS      PROGRESS     TIME                  │
│  ────────────────────────────────────────────────────────────────────────   │
│  PARSE     fix_parser        ✓ Complete  1,247 files  2m ago                │
│  EXPORT    concordance       ↻ Running   ████░░ 67%   now                   │
│            → production_001/             30,521/45,782 records              │
│  SCAN      /data/new_files   ✓ Complete  347 files    15m ago               │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 10.2 Export Detail Panel

```
┌─ EXPORT DETAILS ─────────────────────────────────────────────────────────────┐
│                                                                              │
│  Exporter:    concordance v1.0.0                                            │
│  Status:      Running                                                        │
│  Started:     10:30:15                                                       │
│  Duration:    12m 45s                                                        │
│                                                                              │
│  Input:       3 parquet files (45,782 records)                              │
│  Output:      ./production_001/                                              │
│                                                                              │
│  Progress:    ████████████████░░░░░░░░ 67%                                  │
│               30,521 / 45,782 records                                        │
│               ETA: ~6m remaining                                             │
│                                                                              │
│  Config:                                                                     │
│    bates_prefix: SMITH                                                       │
│    bates_range:  SMITH000001 - SMITH030521 (current)                        │
│    date_format:  MM/DD/YYYY                                                  │
│                                                                              │
│  Output Files:                                                               │
│    loadfile.dat    12.3 MB (writing...)                                     │
│    loadfile.opt    234 KB                                                    │
│    NATIVES/        1,247 files                                               │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 11. Implementation Phases

### Phase 1: Core Infrastructure
- [ ] `ExportResult` and `ExportContext` types
- [ ] Exporter discovery and registration
- [ ] `casparian export` CLI command
- [ ] Job tracking for exports
- [ ] Generic `csv` exporter

### Phase 2: Legal Exporters
- [ ] `concordance` exporter (DAT + OPT)
- [ ] Bates number generation
- [ ] Family relationship tracking
- [ ] Native file handling

### Phase 3: Additional Formats
- [ ] `fhir-r4` exporter
- [ ] `kml` / `geojson` exporters
- [ ] `bloomberg-tca` exporter

### Phase 4: Advanced Features
- [ ] Incremental exports
- [ ] Export resume/checkpoint
- [ ] Custom exporter hot-reload
- [ ] TUI export wizard

---

## 12. Open Questions

1. **Should exporters have schema contracts like parsers?**
   - Pro: Governance, validation
   - Con: Output formats often have loose schemas

2. **How to handle exporter dependencies?**
   - Some exporters need libraries (lxml for KML, fhir.resources for FHIR)
   - Use same UV venv approach as parsers?

3. **Should exports be reversible?**
   - Can we import a Concordance DAT back into Casparian?
   - Useful for round-trip testing

4. **Multi-format exports?**
   - `casparian export concordance,relativity --input ...`
   - Or separate commands?

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 1.0 | Initial specification |
