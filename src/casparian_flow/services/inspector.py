
"""
FileInspector: Stateless profiling of file content.
Design:
- Functions, not classes (Mechanism over Policy).
- Explicit IO boundaries.
- Returns PODs (FileProfile).
"""
import os
import mimetypes
from pathlib import Path
from typing import Tuple

# Internal Imports
from casparian_flow.services.ai_types import (
    FileProfile, 
    FileType, 
    HEAD_Sample
)

# Constants
SAMPLE_SIZE_BYTES = 16 * 1024  # 16KB Header

def _detect_file_type(header: bytes, ext: str) -> FileType:
    """
    Determine file type from Magic Numbers (Signature) first, Extension second.
    """
    # 1. Magic Numbers
    if header.startswith(b"PAR1"):
        return FileType.BINARY_PARQUET
    if header.startswith(b"%PDF"):
        return FileType.BINARY_PDF
    if header.startswith(b"PK\x03\x04"):
        return FileType.BINARY_ZIP
        
    # 2. Heuristic for Text
    try:
        header.decode('utf-8')
        # If successfully decoded, likely text. Check validation.
        stripped = ext.lower().strip()
        if stripped == ".json":
            return FileType.TEXT_JSON
        if stripped == ".xml":
            return FileType.TEXT_XML
        return FileType.TEXT_CSV # Default text assumption
    except UnicodeDecodeError:
        pass
        
    # 3. Binary Fallback
    if ext.lower() in [".xls", ".xlsx"]:
        return FileType.BINARY_EXCEL
        
    return FileType.UNKNOWN

def _sample_head(path: str, size: int) -> Tuple[bytes, str]:
    """
    Read fixed-size head safely.
    Returns (raw_bytes, encoding_name).
    """
    raw = b""
    try:
        with open(path, "rb") as f:
            raw = f.read(size)
    except Exception:
        # If open fails (permissions, etc), we return empty.
        # The caller handles valid/invalid profiles.
        return b"", "unknown"
        
    # Detect encoding
    import chardet # Lazy import
    det = chardet.detect(raw)
    enc = det.get('encoding', 'utf-8') or 'utf-8' # Fallback
    
    return raw, enc

def profile_file(path: str) -> FileProfile:
    """
    Main Entry Point.
    Profiles a file on disk. 
    NO SIDE EFFECTS.
    """
    p_obj = Path(path)
    if not p_obj.exists():
        raise FileNotFoundError(f"Path not found: {path}")
        
    total_size = p_obj.stat().st_size
    name, ext = os.path.splitext(path)
    
    # 1. Sample Head (Blocking IO)
    raw_head, enc = _sample_head(path, SAMPLE_SIZE_BYTES)
    
    # 2. ID Type
    ftype = _detect_file_type(raw_head, ext)
    
    # 3. Advanced Hints (Adaptive Strategy)
    hints = {}
    
    # Dispatch usage of specialized libraries if needed
    if ftype == FileType.BINARY_PDF:
        try:
            # Only import if needed
            from pypdf import PdfReader
            reader = PdfReader(path)
            meta = reader.metadata
            if meta:
                hints["pdf_meta"] = {k:str(v) for k,v in meta.items()}
            # Extract text from first page only
            if len(reader.pages) > 0:
                hints["page_0_text"] = reader.pages[0].extract_text()
        except Exception as e:
            hints["error"] = str(e)
            
    elif ftype == FileType.BINARY_PARQUET:
        try:
            import pyarrow.parquet as pq
            meta = pq.read_metadata(path)
            hints["schema"] = str(meta.schema)
            hints["num_rows"] = meta.num_rows
        except Exception as e:
            hints["error"] = str(e)
            
    # Construct POD
    sample_pod = HEAD_Sample(
        size_bytes=len(raw_head),
        data=raw_head,
        encoding_detected=enc
    )
    
    return FileProfile(
        path=str(path),
        file_type=ftype,
        total_size=total_size,
        head_sample=sample_pod,
        metadata_hints=hints
    )
