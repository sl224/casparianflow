# src/casparian_flow/services/ai_types.py
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
    marker: str = "HEAD"
    size_bytes: int = 0
    data: bytes = field(default_factory=bytes)
    encoding_detected: str = "utf-8"

@dataclass(frozen=True, slots=True)
class FileProfile:
    path: str
    file_type: FileType
    total_size: int
    head_sample: HEAD_Sample
    metadata_hints: Dict[str, Any] = field(default_factory=dict)

@dataclass(frozen=True, slots=True)
class ColumnDef:
    name: str
    target_type: str
    is_nullable: bool = True
    description: Optional[str] = None

@dataclass(frozen=True, slots=True)
class TableDefinition:
    """A definition for a single output table."""
    topic_name: str
    columns: List[ColumnDef]
    description: str = ""

@dataclass(frozen=True, slots=True)
class SchemaProposal:
    """
    The 'Intent' of the plugin.
    Strictly multi-table structure.
    """
    file_type_inferred: str
    tables: List[TableDefinition]
    read_strategy: str
    reasoning: str = ""

@dataclass(frozen=True, slots=True)
class PluginCode:
    filename: str
    source_code: str
    imports: List[str]
    entry_point: str = "Handler"