#%%
import sqlite3
import os

# Set the current working directory to the directory of this script.
# This is useful when running from a different directory (e.g., in a Jupyter cell or IDE).
try:
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
except NameError:
    print("Could not change directory. '__file__' not defined. This is expected in some interactive environments.")

from sqlalchemy import (
    create_engine,
    Table, 
    Column,
    MetaData,
    Integer,
    Text,
)
from pathlib import Path
from dataclasses import dataclass

FileTable = Table(
    "file",
    MetaData(),
    Column("file_id", Integer, primary_key=True, autoincrement=True),
    Column("file_path", Text),
    Column("filesize_bytes", Integer),
)

@dataclass
class FileRecord:
    file_path: str 
    filesize_bytes:int


skip_dirs = {}
skip_files = {}

def is_skip_dir(check_dir):
    return check_dir in skip_dirs

def is_skip_file(check_file):
    return check_file in skip_files

def scan(dirname):
    engine = create_engine("sqlite:///./test.db")
    FileTable.metadata.create_all(engine)

    rows = []
    for cur_dir, dirs, files in os.walk(dirname):
        if is_skip_dir(cur_dir): continue
        for file in files:
            if is_skip_file(file): continue
            cur_path = Path(cur_dir) / file
            if not cur_path.exists():
                print(f'{cur_path} does not exist...skipping')
                continue
            stat_result = cur_path.stat()
            rec = { 
                "file_path": str(cur_path),
                "filesize_bytes": stat_result.st_size
            }
            rows.append(rec)

    with engine.begin() as conn: 
        result = conn.execute(FileTable.insert(), rows)
        print(f"Successfully inserted {result.rowcount} rows into test.db.")


if __name__ == '__main__':
    # print(os.getcwd())
    # print('hello')
    db_path = 'test.db'
    try:
        os.remove(db_path)
        print(f"Deleted existing db: {db_path}")
    except Exception as e:
        print(e)
    scan('.')
        
