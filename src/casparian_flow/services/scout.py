import hashlib
import logging
from pathlib import Path
from datetime import datetime
from sqlalchemy.orm import Session
from casparian_flow.db.models import FileMetadata, FileHashRegistry, ProcessingJob, SourceRoot, StatusEnum
from casparian_flow.services.fs_engine import ParallelFileScanner
from casparian_flow.db.base_session import SessionLocal

logger = logging.getLogger(__name__)

def calculate_file_hash(filepath: Path, chunk_size=8192) -> str:
    """Calculates SHA256 hash of a file."""
    hasher = hashlib.sha256()
    try:
        with open(filepath, "rb") as f:
            while chunk := f.read(chunk_size):
                hasher.update(chunk)
        return hasher.hexdigest()
    except OSError as e:
        logger.error(f"Failed to hash {filepath}: {e}")
        return None

class Scout:
    def __init__(self, db: Session):
        self.db = db
        self.scanner = ParallelFileScanner()

    def scan_source(self, source_root: SourceRoot):
        """
        Scans a SourceRoot and registers files.
        """
        root_path = Path(source_root.path)
        if not root_path.exists():
            logger.error(f"Source root not found: {root_path}")
            return

        logger.info(f"Scouting {root_path}...")

        def filter_file(entry) -> bool:
            # Skip hidden files and temporary files
            return not entry.name.startswith(".")

        def process_file(filepath: Path):
            try:
                self._register_file(filepath, source_root)
            except Exception as e:
                logger.error(f"Error processing {filepath}: {e}")

        self.scanner.walk(root_path, filter_file, process_file)
        self.db.commit()

    def _register_file(self, filepath: Path, source_root: SourceRoot):
        # 1. Calculate Hash
        content_hash = calculate_file_hash(filepath)
        if not content_hash:
            return

        stat = filepath.stat()
        
        # 2. Register Hash (Deduplication)
        hash_entry = self.db.query(FileHashRegistry).filter_by(content_hash=content_hash).first()
        if not hash_entry:
            hash_entry = FileHashRegistry(
                content_hash=content_hash,
                size_bytes=stat.st_size
            )
            self.db.add(hash_entry)
            self.db.flush() # Ensure it's available for FK

        # 3. Register/Update FileMetadata
        rel_path = str(filepath.relative_to(source_root.path))
        
        file_meta = self.db.query(FileMetadata).filter_by(
            source_root_id=source_root.id,
            rel_path=rel_path
        ).first()

        is_new_content = False

        if not file_meta:
            file_meta = FileMetadata(
                source_root_id=source_root.id,
                rel_path=rel_path,
                filename=filepath.name,
                size_bytes=stat.st_size,
                modified_time=datetime.fromtimestamp(stat.st_mtime),
                content_hash=content_hash
            )
            self.db.add(file_meta)
            is_new_content = True
        else:
            # Check if content changed
            if file_meta.content_hash != content_hash:
                file_meta.content_hash = content_hash
                file_meta.size_bytes = stat.st_size
                file_meta.modified_time = datetime.fromtimestamp(stat.st_mtime)
                is_new_content = True
            
            file_meta.last_seen_time = datetime.now()

        self.db.flush()

        # 4. Queue Job if new content
        if is_new_content:
            self._queue_job(file_meta)

    def _queue_job(self, file_meta: FileMetadata):
        # For now, we queue for ALL plugins that are "wired" (or just a default one for now)
        # The user said "Push new files to ProcessingJob table"
        # We need a plugin name. Let's assume a default or look up configuration.
        # For the smoke test, we might need a dummy plugin.
        
        # Let's check if there are any PluginConfigs.
        # If not, we might skip or log.
        # For now, I'll query all PluginConfigs and create a job for each.
        
        # If no plugins configured, maybe we can't queue?
        # The user said "Refine Wiring: Currently, plugins might default to .parquet files."
        
        # I'll fetch all active plugins from PluginConfig.
        from casparian_flow.db.models import PluginConfig
        plugins = self.db.query(PluginConfig).all()
        
        if not plugins:
            logger.warning(f"No plugins configured. File {file_meta.filename} registered but not queued.")
            return

        for plugin in plugins:
            job = ProcessingJob(
                file_id=file_meta.id,
                plugin_name=plugin.plugin_name,
                status=StatusEnum.QUEUED
            )
            self.db.add(job)
            logger.info(f"Queued job for {file_meta.filename} -> {plugin.plugin_name}")

if __name__ == "__main__":
    # Standalone run for testing
    logging.basicConfig(level=logging.INFO)
    db = SessionLocal()
    scout = Scout(db)
    
    # Example usage:
    # root = db.query(SourceRoot).first()
    # if root:
    #     scout.scan_source(root)
