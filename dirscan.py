#%%
import os
from pathlib import Path
import pathspec 
from datetime import datetime

try:
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
except NameError:
    pass

from metadata_tables import FolderRecord, FileRecord, Base, ProcessingLog
from sqlalchemy import insert, update

# Internal
import sql_io
from global_config import settings 


class JobUpdater:
    def __init__(self, eng):
        self.status= "QUEUED"
        self.pid = None
        self.eng = eng


    def init_status(self):
        rec = ProcessingLog(
            processing_start = datetime.now(),
            processing_end = None, 
            process_status = "QUEUED",
            status_updated_at = datetime.now(),
        )

        with self.eng.begin() as conn:
            result = conn.execute(ProcessingLog.insert(), rec.to_dict())
            if result.rowcount == 0:
                raise Exception("Error could not insert status in to processing table")
            self.pid = result.inserted_primary_key[0]
            print(f"Successfully inserted {result.rowcount} rows into processing status.")

    def update_status(self, status, end=None):
        rec = ProcessingLog(
            processing_start = datetime.now(),
            processing_end = end, 
            process_status = status,
            status_updated_at = datetime.now(),
        )

        with self.eng.begin() as conn:
            stmt = (
                update(ProcessingLog)
                .where(ProcessingLog.process_id == self.pid)
                .values(process_status=status)
            )
            result = conn.execute(stmt)
            if result.rowcount == 0:
                raise Exception("Error could not insert status in to processing table")
            # print(f"Successfully updated {result.rowcount} rows into processing status.")




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
    lines.append('global_config.yaml')

    return pathspec.GitIgnoreSpec.from_lines('gitwildmatch', lines)

def scan(engine, j, dirname, ignore_file_name=".scanignore"):
    scan_root = Path(dirname).resolve()
    print("scanning... ", scan_root)
    ignore_file_path = scan_root / ignore_file_name
    spec = load_pathspec(ignore_file_path, ignore_file_name)
    
    rows_to_insert = []
    folders_to_insert = []
    file_parents_map = {}
    j.init_status()
    
    for cur_dir, dirs, files in os.walk(scan_root, topdown=True):
        j.update_status("PROCESSING")
        current_rel_dir = Path(cur_dir).relative_to(scan_root)
        phash = hash(current_rel_dir)
        folders_to_insert.append({'folder_path': str(current_rel_dir)})

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
            file_parents_map[hash(cur_path)] = phash 
            
            if spec.match_file(rel_path):
                print(f"Skipping File: {rel_path}")
                continue

            try:
                stat_result = cur_path.stat()
                rows_to_insert.append(
                    {
                        "file_path": str(cur_path),
                        "file_name": file,
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
        folder_ins_stmt = (
            insert(FolderRecord)
            .values(folders_to_insert)
            .returning(
                FolderRecord.folder_id,
                FolderRecord.folder_path
            ) 
        )
        result = conn.execute(folder_ins_stmt)
        print(f"Successfully inserted {result.rowcount} rows into folder")
        path_id_map = {hash(Path(fpath)): fid for fid, fpath in result.all()}
        for row in rows_to_insert:
            parent_hash = file_parents_map[hash(row['file_path'])]
            row['folder_id'] = path_id_map[parent_hash]

        result = conn.execute(FileRecord.insert(), rows_to_insert)
        print(f"Successfully inserted {result.rowcount} rows into {settings.db.db_location}.")
    j.update_status(status="COMPLETE", end=datetime.now())

if __name__ == '__main__':

    # The config file points duckdb to 'test.db', so this path is correct
    settings.load('global_config.yaml')
    db_path = settings.db.db_location
    print(f"Using DB: {db_path}")
    if os.path.exists(db_path):
        os.remove(db_path)
        print(f"Successfully deleted existing db: {db_path}")

    engine = sql_io.get_engine()
    Base.metadata.create_all(engine)

    j = JobUpdater(engine)
    scan_dir = '.'
    print(f"Running scan on {scan_dir}")
    scan(engine, j, scan_dir)