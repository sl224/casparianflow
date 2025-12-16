
"""
Core Data Structures for the AI Plugin Generation Workflow.
Designed as Plain Old Data (POD) structures.
"""
from dataclasses import dataclass, field
from typing import List, Optional, Dict, Any
from enum import Enum

class FileType(Enum):
    UNKNOWN = 0
    TEXT_CSV = 1
    TEXT_JSON = 2
    TEXT_XML = 3
    BINARY_PARQUET = 4
    BINARY_EXCEL = 5
    BINARY_PDF = 6
    BINARY_ZIP = 7

@dataclass(frozen=True, slots=True)
class HEAD_Sample:
    """
    Represents a raw byte sample of the file's head.
    Safe for passing across boundaries.
    """
    marker: str = "HEAD"
    size_bytes: int = 0
    data: bytes = field(default_factory=bytes)
    encoding_detected: str = "utf-8"

@dataclass(frozen=True, slots=True)
class FileProfile:
    """
    A profile of a file on disk, sufficient to infer its schema
    without reading the entire content.
    """
    path: str
    file_type: FileType
    total_size: int
    head_sample: HEAD_Sample
    
    # Format-specific metadata (e.g. Parquet Schema string, PDF text dump)
    # Kept as raw dictionaries/strings to avoid tight coupling to libraries
    metadata_hints: Dict[str, Any] = field(default_factory=dict) 

@dataclass(frozen=True, slots=True)
class ColumnDef:
    """Explicit column definition."""
    name: str
    target_type: str  # 'int', 'float', 'string', 'datetime'
    is_nullable: bool = True
    description: Optional[str] = None

@dataclass(frozen=True, slots=True)
class SchemaProposal:
    """
    The 'Intent' of the plugin.
    Must be approved by user before code generation.
    """
    file_type_inferred: str
    target_topic: str
    columns: List[ColumnDef]
    read_strategy: str # e.g. "pandas_csv", "pyarrow_parquet"
    reasoning: str = ""

@dataclass(frozen=True, slots=True)
class PluginCode:
    """
    The final generated artifact.
    """
    filename: str
    source_code: str
    imports: List[str]
    entry_point: str = "Handler"
