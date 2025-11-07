# %%
import os
from pathlib import Path
import pathspec
from datetime import datetime
import logging

# --- TQDM IMPORT ---
from tqdm import tqdm

# --- Import the logging redirect context manager ---
from tqdm.contrib.logging import logging_redirect_tqdm

try:
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
except NameError:
    pass

# Assuming these are in a local file metadata_tables.py
from metadata_tables import FolderRecord, FileRecord, Base, ProcessingLog
from sqlalchemy import insert, update

# Internal
import sql_io
from global_config import settings
# --- REMOVED: No longer need logging_setup ---


# Get a logger for this specific module
logger = logging.getLogger(__name__)


class JobUpdater:
    def __init__(self, eng, name):
        self.status = "QUEUED"
        self.pid = None
        self.eng = eng
        self.name = name
        # Get a logger for this class/module
        self.logger = logging.getLogger("JobUpdater")

    def init_status(self):
        rec = {
            "name": self.name,
            "processing_start": datetime.now(),
            "processing_end": None,
            "process_status": "QUEUED",
        }

        with self.eng.begin() as conn:
            result = conn.execute(ProcessingLog.insert(), rec)
            if result.rowcount == 0:
                self.logger.error(
                    "Could not insert initial status in to processing table"
                )
                raise Exception("Error could not insert status in to processing table")
            self.pid = result.inserted_primary_key[0]
            self.logger.info(
                f"Successfully inserted status for new process_id: {self.pid}"
            )

    def update_status(self, status):
        with self.eng.begin() as conn:
            stmt_values = {"process_status": status}

            stmt = (
                update(ProcessingLog)
                .where(ProcessingLog.id == self.pid)
                .values(**stmt_values)
            )
            result = conn.execute(stmt)
            if result.rowcount == 0:
                self.logger.error(f"Could not update status for process_id: {self.pid}")
                raise Exception("Error could not update status in to processing table")
            self.logger.debug(
                f"Successfully updated status to {status} for process_id: {self.pid}"
            )


def load_pathspec(ignore_file_path, ignore_file_name):
    """
    Loads the ignore file, adds the ignore file itself to the patterns,
    and returns a compiled PathSpec object.
    """
    lines = []
    try:
        with open(ignore_file_path, "r") as f:
            lines = f.readlines()
    except FileNotFoundError:
        logger.warning(
            f"Ignore file '{ignore_file_path}' not found. Scanning all files."
        )

    lines.append(ignore_file_name)
    lines.append("global_config.yaml")

    return pathspec.GitIgnoreSpec.from_lines("gitwildmatch", lines)


def scan(engine, j: JobUpdater, dirname, ignore_file_name=".scanignore"):
    scan_root = Path(dirname).resolve()
    logger.info(f"Starting scan: {scan_root}")
    ignore_file_path = scan_root / ignore_file_name
    spec = load_pathspec(ignore_file_path, ignore_file_name)

    rows_to_insert = []
    folders_to_insert = []
    file_parents_map = {}

    try:
        j.init_status()
    except Exception as e:
        logger.critical(
            f"Failed to initialize job status. Aborting scan.", exc_info=True
        )
        return

    # --- TQDM: TWO BARS ---
    folder_bar = tqdm(desc="Folders", unit=" folder", dynamic_ncols=True, position=0)
    file_bar = tqdm(desc="Files", unit=" file", dynamic_ncols=True, position=1)

    # --- Wrap the entire progress bar section ---
    with logging_redirect_tqdm(), folder_bar, file_bar:
        for cur_dir, dirs, files in os.walk(scan_root, topdown=True):
            try:
                j.update_status("PROCESSING")
            except Exception as e:
                logger.error(
                    f"Failed to update job status to PROCESSING.", exc_info=True
                )

            current_rel_dir = Path(cur_dir).relative_to(scan_root)

            phash = hash(current_rel_dir)
            folders_to_insert.append(
                {"folder_path": str(current_rel_dir), "process_id": j.pid}
            )

            if spec:
                dirs_to_check = [(d, (current_rel_dir / d).as_posix()) for d in dirs]
                fil_dirs = []
                for d, rel_path in dirs_to_check:
                    if not spec.match_file(rel_path) and not spec.match_file(
                        rel_path + "/"
                    ):
                        fil_dirs.append(d)
                    else:
                        logger.debug(f"Skipping directory: {rel_path}")
                dirs[:] = fil_dirs

            for file in files:
                file_bar.update(1)

                cur_path = Path(cur_dir) / file
                rel_path = cur_path.relative_to(scan_root).as_posix()

                # --- REVERTED: Hash the Path object ---
                file_parents_map[hash(cur_path)] = phash

                if spec.match_file(rel_path):
                    logger.debug(f"Skipping ignored file: {rel_path}")
                    continue

                try:
                    stat_result = cur_path.stat()
                    rows_to_insert.append(
                        {
                            "process_id": j.pid,
                            "file_path": str(cur_path),
                            "file_name": file,
                            "filesize_bytes": stat_result.st_size,
                        }
                    )
                except FileNotFoundError:
                    logger.warning(f"{cur_path} was listed but not found. Skipping.")
                except OSError as e:
                    logger.error(f"Error stating file {cur_path}: {e}. Skipping.")

            folder_bar.update(1)

    if not rows_to_insert:
        logger.warning("No files found to insert. Ending job.")
        j.update_status(status="COMPLETE_NO_FILES")
        return

    try:
        with engine.begin() as conn:
            folder_bar.set_description("Inserting Folders")
            folder_bar.set_postfix_str("")  # Clear postfix
            file_bar.set_description("")
            file_bar.set_postfix_str("")

            folder_ins_stmt = (
                insert(FolderRecord)
                .values(folders_to_insert)
                .returning(FolderRecord.id, FolderRecord.folder_path)
            )
            returned_rows = conn.execute(folder_ins_stmt).all()
            logger.info(
                f"Successfully inserted {len(returned_rows)} rows into the folder table"
            )

            folder_bar.set_description("Mapping file paths")

            # --- REVERTED: Hash the Path(fpath) ---
            path_id_map = {hash(Path(fpath)): fid for fid, fpath in returned_rows}

            for row in rows_to_insert:
                # --- REVERTED: Hash the Path(row["file_path"]) ---
                parent_hash = file_parents_map[hash(Path(row["file_path"]))]
                row["folder_id"] = path_id_map[parent_hash]

            folder_bar.set_description("Inserting Files")
            result = conn.execute(FileRecord.insert(), rows_to_insert)
            logger.info(
                f"Successfully inserted {result.rowcount} rows into the file table"
            )

        j.update_status(status="COMPLETE")
        folder_bar.set_description("Scan Complete!")
        logger.info(f"Scan for process_id {j.pid} completed successfully.")

    except Exception as e:
        logger.critical(
            f"Failed during database insert for process_id {j.pid}.", exc_info=True
        )
        try:
            j.update_status(status="FAILED_DB_INSERT")
        except Exception as update_e:
            logger.error(
                f"Failed to update status to FAILED after DB error.", exc_info=update_e
            )


def main():
    # Program Init
    settings.load("global_config.yaml")

    # --- Use basicConfig instead of setup_logging ---
    log_level = settings.logging.level.upper()
    logging.basicConfig(
        level=log_level, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
    )

    db_path = settings.db.db_location
    if os.path.exists(db_path):
        os.remove(db_path)
        logger.debug(f"Successfully deleted existing db: {db_path}")

    logger.debug(f"Using DB: {db_path}")

    engine = None
    j = None
    try:
        engine = sql_io.get_engine()
        Base.metadata.create_all(engine)

        j = JobUpdater(engine, "Folder Scan")
        scan_dir = "C:/"
        scan(engine, j, scan_dir)

    except Exception as e:
        logger.critical("Scan job failed with an unhandled exception.", exc_info=True)
        if j and j.pid:
            logger.error(f"Attempting to mark job {j.pid} as FAILED.")
            try:
                j.update_status(status="FAILED_UNHANDLED")
            except Exception as final_e:
                logger.error(
                    "Could not even update status to FAILED.", exc_info=final_e
                )


if __name__ == "__main__":
    main()
