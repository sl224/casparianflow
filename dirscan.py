#%%
import os
from pathlib import Path
import pathspec 
from casp_sa_base import Base

# Set the current working directory
try:
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
except NameError:
    pass

from sqlalchemy.orm import Mapped, mapped_column

# Internal
import sql_io
from global_config import settings # <--- ADDED: Import settings instance

class FileRecord(Base):
    __tablename__ = "file"
    file_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    file_path: Mapped[str] = mapped_column(unique=True)
    filesize_bytes: Mapped[int]
    # remove for prod
    __table_args__ = {'extend_existing': True}

# --- New Scan Function ---
def load_pathspec(ignore_file_path, ignore_file_name):
    """
    Loads the ignore file, adds the ignore file itself to the patterns,
    and returns a compiled PathSpec object.
    """
    lines = []
    try:
        with open(ignore_file_path, 'r') as f:
            lines = f.readlines()
    except FileNotFoundError:
        print(f"Warning: Ignore file '{ignore_file_path}' not found. Scanning all files.")
    
    lines.append(ignore_file_name)
    # Also ignore the config file
    lines.append('global_config.yaml')
    return pathspec.PathSpec.from_lines('gitwildmatch', lines)

def scan(dirname, ignore_file_name=".scanignore"):
    engine = sql_io.get_engine()
    scan_root = Path(dirname).resolve()
    ignore_file_path = scan_root / ignore_file_name

    spec = load_pathspec(ignore_file_path, ignore_file_name)
    Base.metadata.create_all(engine)
    
    # --- MODIFICATION: Build dicts directly ---
    rows_to_insert = []
    
    for cur_dir, dirs, files in os.walk(scan_root, topdown=True):
        
        current_rel_dir = Path(cur_dir).relative_to(scan_root)

        if spec:
            dirs_to_check = [
                (d, (current_rel_dir / d).as_posix()) for d in dirs
            ]
            
            dirs[:] = [
                d for d, rel_path in dirs_to_check
                if not spec.match_file(rel_path) and not spec.match_file(rel_path + '/')
            ]

        for file in files:
            cur_path = Path(cur_dir) / file
            rel_path = cur_path.relative_to(scan_root).as_posix()
            
            if spec.match_file(rel_path):
                continue

            try:
                stat_result = cur_path.stat()
                # --- MODIFICATION: Append dict directly ---
                rows_to_insert.append(
                    {
                        "file_path": str(cur_path),
                        "filesize_bytes": stat_result.st_size
                    }
                )
            except FileNotFoundError:
                print(f"Warning: {cur_path} was listed but not found. Skipping.")
            except OSError as e:
                print(f"Error stating file {cur_path}: {e}. Skipping.")

    # --- MODIFICATION: Check new list name ---
    if not rows_to_insert:
        print("No files found to insert.")
        return

    with engine.begin() as conn:
        # This now correctly receives the list of dicts
        result = conn.execute(FileRecord.insert(), rows_to_insert)
        print(f"Successfully inserted {result.rowcount} rows into {settings.db.db_location}.")

#%%
if __name__ == '__main__':

    # The config file points duckdb to 'test.db', so this path is correct
    settings.load('global_config.yaml')
    db_path = settings.db.db_location
    print(f"using {db_path}")

    if os.path.exists(db_path):
        os.remove(db_path)
        print(f"Successfully deleted existing db: {db_path}")
    # if os.path.exists(f"{db_path}.wal"): # DuckDB creates a WAL file
    #     os.remove(f"{db_path}")
        # print(f"Successfully deleted existing db WAL: {db_path}.wal")

    # --- ADDED: Load configuration from file ---
    # This MUST be called before get_engine() is first used.
    
    # Scan the current directory
    # scan('.')