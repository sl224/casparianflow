#%%
import os
from pathlib import Path
import pathspec 
from datetime import datetime
import logging # <-- Import logging

try:
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
except NameError:
    pass

from metadata_tables import FolderRecord, FileRecord, Base, ProcessingLog
from sqlalchemy import insert, update

# Internal
import sql_io
from global_config import settings 
from logging_setup import setup_logging # <-- Import the setup function

# --- NEW ---
# Get a logger for this specific module
# This is a best practice.
logger = logging.getLogger(__name__)

class JobUpdater:
    def __init__(self, eng):
        self.status= "QUEUED"
        self.pid = None
        self.eng = eng
        # --- NEW ---
        # Get a logger for this class/module
        self.logger = logging.getLogger(__name__) 


    def init_status(self):
        rec = { 
            "processing_start" : datetime.now(),
            "processing_end": None, 
            "process_status": "QUEUED",
        }

        with self.eng.begin() as conn:
            result = conn.execute(ProcessingLog.insert(), rec)
            if result.rowcount == 0:
                # --- MODIFIED ---
                self.logger.error("Could not insert initial status in to processing table")
                raise Exception("Error could not insert status in to processing table")
            self.pid = result.inserted_primary_key[0]
            # --- MODIFIED ---
            self.logger.info(f"Successfully inserted status for new process_id: {self.pid}")

    def update_status(self, status):
        with self.eng.begin() as conn:
            stmt_values = {"process_status": status}

            stmt = (
                update(ProcessingLog)
                .where(ProcessingLog.process_id == self.pid)
                .values(**stmt_values) # <-- Use updated values
            )
            result = conn.execute(stmt)
            if result.rowcount == 0:
                # --- MODIFIED ---
                self.logger.error(f"Could not update status for process_id: {self.pid}")
                raise Exception("Error could not update status in to processing table")
            # --- MODIFIED (use debug for verbose info) ---
            self.logger.debug(f"Successfully updated status to {status} for process_id: {self.pid}")

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
        # --- MODIFIED ---
        logger.warning(f"Ignore file '{ignore_file_path}' not found. Scanning all files.")
    
    lines.append(ignore_file_name)
    lines.append('global_config.yaml')

    return pathspec.GitIgnoreSpec.from_lines('gitwildmatch', lines)

def scan(engine, j: JobUpdater, dirname, ignore_file_name=".scanignore"):
    scan_root = Path(dirname).resolve()
    # --- MODIFIED ---
    logger.info(f"Starting scan on: {scan_root}")
    ignore_file_path = scan_root / ignore_file_name
    spec = load_pathspec(ignore_file_path, ignore_file_name)
    
    rows_to_insert = []
    folders_to_insert = []
    file_parents_map = {}
    
    try:
        j.init_status() # <-- Now we can catch an init failure
    except Exception as e:
        logger.critical(f"Failed to initialize job status. Aborting scan.", exc_info=True)
        return # Stop the scan

    for cur_dir, dirs, files in os.walk(scan_root, topdown=True):
        try:
            j.update_status("PROCESSING")
        except Exception as e:
            # If this fails, log it but don't stop the whole scan
            logger.error(f"Failed to update job status to PROCESSING.", exc_info=True)

        current_rel_dir = Path(cur_dir).relative_to(scan_root)
        phash = hash(current_rel_dir)
        folders_to_insert.append({
            'folder_path': str(current_rel_dir),
            'process_id': j.pid 
        })

        if spec:
            dirs_to_check = [
                (d, (current_rel_dir / d).as_posix()) for d in dirs
            ]
            fil_dirs = []
            for d, rel_path in dirs_to_check:
                if not spec.match_file(rel_path) and not spec.match_file(rel_path + '/'):
                    fil_dirs.append(d)
                else:
                    # --- MODIFIED ---
                    logger.debug(f"Skipping directory: {rel_path}")
            dirs[:] = fil_dirs 

        for file in files:
            cur_path = Path(cur_dir) / file
            rel_path = cur_path.relative_to(scan_root).as_posix()
            file_parents_map[hash(cur_path)] = phash 
            
            if spec.match_file(rel_path):
                # --- MODIFIED ---
                logger.debug(f"Skipping ignored file: {rel_path}")
                continue

            try:
                stat_result = cur_path.stat()
                rows_to_insert.append(
                    {
                        "process_id": j.pid,
                        "file_path": str(cur_path),
                        "file_name": file,
                        "filesize_bytes": stat_result.st_size
                    }
                )
            except FileNotFoundError:
                # --- MODIFIED ---
                logger.warning(f"{cur_path} was listed but not found. Skipping.")
            except OSError as e:
                # --- MODIFIED ---
                logger.error(f"Error stating file {cur_path}: {e}. Skipping.")

    if not rows_to_insert:
        # --- MODIFIED ---
        logger.warning("No files found to insert. Ending job.")
        j.update_status(status="COMPLETE_NO_FILES", end=datetime.now()) # <-- Use a more specific status
        return

    try:
        with engine.begin() as conn:
            folder_ins_stmt = (
                insert(FolderRecord)
                .values(folders_to_insert)
                .returning(
                    FolderRecord.folder_id,
                    FolderRecord.folder_path
                ) 
            )
            result = conn.execute(folder_ins_stmt)
            # --- MODIFIED ---
            logger.info(f"Successfully inserted {result.rowcount} rows into folder")
            
            path_id_map = {hash(Path(fpath)): fid for fid, fpath in result.all()}
            for row in rows_to_insert:
                parent_hash = file_parents_map[hash(row['file_path'])]
                row['folder_id'] = path_id_map[parent_hash]

            result = conn.execute(FileRecord.insert(), rows_to_insert)
            # --- MODIFIED ---
            logger.info(f"Successfully inserted {result.rowcount} file rows into {settings.db.db_location}.")
        
        j.update_status(status="COMPLETE", end=datetime.now())
        logger.info(f"Scan for process_id {j.pid} completed successfully.")
    
    except Exception as e:
        logger.critical(f"Failed during database insert for process_id {j.pid}.", exc_info=True)
        try:
            j.update_status(status="FAILED_DB_INSERT")
        except Exception as update_e:
            logger.error(f"Failed to update status to FAILED after DB error.", exc_info=update_e)


if __name__ == '__main__':
    
    settings.load('global_config.yaml')
    
    # --- NEW: Call the setup function ONCE ---
    setup_logging(settings)
    
    db_path = settings.db.db_location
    logger.info(f"Using DB: {db_path}") # <-- Use logger
    if os.path.exists(db_path):
        os.remove(db_path)
        logger.info(f"Successfully deleted existing db: {db_path}") # <-- Use logger

    # --- NEW: Add a top-level try/except block ---
    # This catches any unhandled crashes
    engine = None
    j = None
    try:
        engine = sql_io.get_engine()
        Base.metadata.create_all(engine)

        j = JobUpdater(engine)
        scan_dir = '/Users/shan' 
        logger.info(f"Running scan on {scan_dir}") # <-- Use logger
        scan(engine, j, scan_dir)

    except Exception as e:
        logger.critical("Scan job failed with an unhandled exception.", exc_info=True)
        if j and j.pid:
            logger.error(f"Attempting to mark job {j.pid} as FAILED.")
            try:
                j.update_status(status="FAILED_UNHANDLED", end=datetime.now())
            except Exception as final_e:
                logger.error(f"Could not even update status to FAILED.", exc_info=final_e)