import hashlib
import logging
from pathlib import Path, PurePath
from datetime import datetime
from sqlalchemy.orm import Session
from casparian_flow.db.models import (
    FileLocation, FileVersion, FileHashRegistry, ProcessingJob, 
    SourceRoot, StatusEnum, RoutingRule, PluginConfig
)
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

    def _calculate_tags(self, rel_path: str) -> str:
        """Match path against active RoutingRules and return comma-separated tags."""
        # 1. Fetch rules (Cached in memory for performance ideally)
        rules = self.db.query(RoutingRule).order_by(RoutingRule.priority.desc()).all()
        
        path_obj = PurePath(rel_path)
        tags = set()
        
        for rule in rules:
            # Use python's pathlib match
            if path_obj.match(rule.pattern):
                tags.add(rule.tag)
        
        # Always add extension as a default tag? Optional, but useful.
        # tags.add(path_obj.suffix.lstrip("."))
        
        return ",".join(sorted(list(tags)))

    def _register_file(self, filepath: Path, source_root: SourceRoot):
        """
        Register a file using the versioning scheme.
        Creates immutable FileVersion records instead of updating in place.
        """
        # 1. Calculate Hash
        content_hash = calculate_file_hash(filepath)
        if not content_hash:
            return

        stat = filepath.stat()
        rel_path = str(filepath.relative_to(source_root.path))
        
        # 2. Register Hash (Deduplication)
        hash_entry = self.db.query(FileHashRegistry).filter_by(content_hash=content_hash).first()
        if not hash_entry:
            hash_entry = FileHashRegistry(
                content_hash=content_hash,
                size_bytes=stat.st_size
            )
            self.db.add(hash_entry)
            self.db.flush()

        # 3. Get or Create FileLocation (The Container)
        location = self.db.query(FileLocation).filter_by(
            source_root_id=source_root.id,
            rel_path=rel_path
        ).first()

        if not location:
            location = FileLocation(
                source_root_id=source_root.id,
                rel_path=rel_path,
                filename=filepath.name
            )
            self.db.add(location)
            self.db.flush()
            logger.debug(f"Created new FileLocation for {rel_path}")

        # 4. Check Current Version
        current_version = None
        if location.current_version_id:
            current_version = self.db.query(FileVersion).get(location.current_version_id)

        # 5. Detect Change (new file or content changed)
        needs_new_version = False
        if not current_version:
            needs_new_version = True
            logger.debug(f"New file detected: {rel_path}")
        elif current_version.content_hash != content_hash:
            needs_new_version = True
            logger.info(f"File content changed: {rel_path} (old hash: {current_version.content_hash[:8]}..., new hash: {content_hash[:8]}...)")

        # 6. Create New Version if needed
        if needs_new_version:
            # Calculate tags for this version
            tags_str = self._calculate_tags(rel_path)
            
            new_version = FileVersion(
                location_id=location.id,
                content_hash=content_hash,
                size_bytes=stat.st_size,
                modified_time=datetime.fromtimestamp(stat.st_mtime),
                applied_tags=tags_str
            )
            self.db.add(new_version)
            self.db.flush()

            # Update pointer to latest version
            location.current_version_id = new_version.id
            location.last_seen_time = datetime.now()

            # Queue job based on TAGS
            self._queue_job(new_version, tags_str)
        else:
            # Same content, just update last_seen
            location.last_seen_time = datetime.now()

    def _queue_job(self, file_version: FileVersion, tags_str: str):
        """
        Queue processing jobs for a specific file version.
        Only queues for plugins that subscribe to the file's tags.
        """
        if not tags_str:
            logger.debug(f"No tags for file version {file_version.id}, skipping queue.")
            return

        file_tags = set(tags_str.split(","))
        
        plugins = self.db.query(PluginConfig).all()
        
        if not plugins:
            logger.warning(f"No plugins configured. File version {file_version.id} registered but not queued.")
            return

        queued_count = 0
        for plugin in plugins:
            # Parse plugin subscriptions
            if not plugin.subscription_tags:
                continue
                
            sub_tags = set([t.strip() for t in plugin.subscription_tags.split(",")])
            
            # Intersection: If file has tag 'A' and plugin wants 'A', match!
            if file_tags.intersection(sub_tags):
                job = ProcessingJob(
                    file_version_id=file_version.id,
                    plugin_name=plugin.plugin_name,
                    status=StatusEnum.QUEUED
                )
                self.db.add(job)
                queued_count += 1
                
                # Get filename from location for logging
                location = self.db.query(FileLocation).get(file_version.location_id)
                logger.info(f"Queued job for {location.filename} -> {plugin.plugin_name} (Tags: {tags_str} matched {plugin.subscription_tags})")

        if queued_count == 0:
            logger.debug(f"No plugins matched tags {tags_str} for version {file_version.id}")

if __name__ == "__main__":
    # Standalone run for testing
    logging.basicConfig(level=logging.INFO)
    db = SessionLocal()
    scout = Scout(db)