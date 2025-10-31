#%%
import os
from pathlib import Path
import pathspec  # <--- Import pathspec
from casp_sa_base import Base

# Set the current working directory
try:
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
except NameError:
    pass

from sqlalchemy import (
    create_engine,
)
from sqlalchemy.orm import Mapped, mapped_column

class FileRecord(Base):
    __tablename__ = "file"
    file_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    file_path: Mapped[str] = mapped_column(unique=True)
    filesize_bytes: Mapped[int]

# --- New Scan Function ---
def load_pathspec(ignore_file_path):
    """Loads the .scanignore file and returns a compiled PathSpec object."""
    try:
        with open(ignore_file_path, 'r') as f:
            lines = f.readlines()
        # Use 'gitwildmatch' to get the exact .gitignore syntax
        return pathspec.PathSpec.from_lines('gitwildmatch', lines)
    except FileNotFoundError:
        print(f"Warning: Ignore file '{ignore_file_path}' not found. Scanning all files.")
        return None

def scan(dirname, ignore_file_name=".scanignore"):
    engine = create_engine("sqlite:///./test.db")
    Base.metadata.create_all(engine)

    # Convert dirname to an absolute path for robust relative-path calculations
    scan_root = Path(dirname).resolve()
    ignore_file_path = scan_root / ignore_file_name

    # 1. Load the ignore spec ONCE
    spec = load_pathspec(ignore_file_path)
    
    file_records = []
    
    # We must use the absolute path for os.walk
    for cur_dir, dirs, files in os.walk(scan_root, topdown=True):
        
        # We need paths relative to the scan_root for matching
        # Example: /home/user/project -> .
        #          /home/user/project/src -> src
        current_rel_dir = Path(cur_dir).relative_to(scan_root)

        # 2. PRUNE DIRECTORIES (This is the most important optimization)
        # We modify 'dirs' IN-PLACE to stop os.walk from descending
        if spec:
            # Check dirs against the spec
            # We use .as_posix() for consistent cross-platform / separators
            dirs_to_check = [
                (d, (current_rel_dir / d).as_posix()) for d in dirs
            ]
            
            # Keep only the dirs that DO NOT match the ignore spec
            # We must also check if the path is a directory (add trailing /)
            dirs[:] = [
                d for d, rel_path in dirs_to_check
                if not spec.match_file(rel_path) and not spec.match_file(rel_path + '/')
            ]

        # 3. SKIP FILES
        for file in files:
            cur_path = Path(cur_dir) / file
            
            # Get the relative path for matching
            rel_path = cur_path.relative_to(scan_root).as_posix()
            
            # Also check the ignore file itself
            if rel_path == ignore_file_name:
                continue
                
            if spec and spec.match_file(rel_path):
                continue

            # If we're here, the file is not ignored
            try:
                stat_result = cur_path.stat()
                file_records.append(
                    FileRecord(
                        file_path=str(cur_path), # Store the full path
                        filesize_bytes=stat_result.st_size
                    )
                )
            except FileNotFoundError:
                print(f"Warning: {cur_path} was listed but not found. Skipping.")
            except OSError as e:
                print(f"Error stating file {cur_path}: {e}. Skipping.")


    if not file_records:
        print("No files found to insert.")
        return

    rows_to_insert = [rec.to_dict() for rec in file_records]

    with engine.begin() as conn:
        result = conn.execute(FileRecord.insert(), rows_to_insert)
        print(f"Successfully inserted {result.rowcount} rows into test.db.")

if __name__ == '__main__':
    db_path = 'test.db'
    if os.path.exists(db_path):
        os.remove(db_path)
        print(f"Successfully deleted existing db: {db_path}")
    
    # Scan the current directory
    scan('.')