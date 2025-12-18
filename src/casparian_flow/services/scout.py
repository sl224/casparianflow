# src/casparian_flow/services/scout.py
import hashlib
import logging
import time
from pathlib import Path, PurePath
from datetime import datetime, timedelta
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import List, Dict, Tuple, Set, Optional, Any

from sqlalchemy.orm import Session
from sqlalchemy import select, insert, text, update
from casparian_flow.db.models import (
    FileLocation,
    FileVersion,
    FileHashRegistry,
    ProcessingJob,
    SourceRoot,
    StatusEnum,
    RoutingRule,
    PluginConfig,
)
from casparian_flow.services.fs_engine import ParallelFileScanner

logger = logging.getLogger(__name__)

# Configurable stability delay for detecting files in transit
STABILITY_DELAY_SECONDS = 0.1


def calculate_hash_and_stat(filepath: Path) -> Optional[Tuple[str, int]]:
    """
    Computes SHA-256 hash and validates file stability.
    Returns (hash, size) or None.
    """
    try:
        # Stability check
        stat1 = filepath.stat()
        time.sleep(STABILITY_DELAY_SECONDS)
        stat2 = filepath.stat()

        if stat1.st_mtime != stat2.st_mtime or stat1.st_size != stat2.st_size:
            logger.debug(f"Skipping in-transit file: {filepath}")
            return None

        hasher = hashlib.sha256()
        with open(filepath, "rb") as f:
            while chunk := f.read(65536):
                hasher.update(chunk)
        return hasher.hexdigest(), stat2.st_size
    except Exception as e:
        logger.warning(f"Skipping {filepath}: {e}")
        return None


def calculate_priority_from_mtime(mtime: float) -> int:
    """QoS Priority based on file age."""
    now = datetime.now()
    file_dt = datetime.fromtimestamp(mtime)
    age = now - file_dt

    if age < timedelta(days=1):
        return 100
    elif age < timedelta(days=7):
        return 50
    else:
        return 10


class InventoryScanner:
    """
    Component 1: The Scanner.
    Responsible for fast I/O inventory of the filesystem.
    Does NOT compute hashes of content.
    Updates `FileLocation` with current mtime/size.
    """
    def __init__(self, db: Session):
        self.db = db
        self.scanner = ParallelFileScanner()
        self.BATCH_SIZE = 2000

    def scan(self, source_root: SourceRoot):
        root_path = Path(source_root.path)
        if not root_path.exists():
            logger.error(f"Source root not found: {root_path}")
            return

        logger.info(f"Inventory Scan: {root_path}")

        def filter_file(entry):
            return not entry.name.startswith(".")

        batch = []
        
        # Parallel Walk (I/O Bound)
        for path_obj in self.scanner.walk(root_path, filter_file):
            try:
                stat = path_obj.stat()
                rel_path = str(path_obj.relative_to(root_path))
                
                batch.append({
                    "rel_path": rel_path,
                    "filename": path_obj.name,
                    "mtime": stat.st_mtime,
                    "size": stat.st_size
                })
                
                if len(batch) >= self.BATCH_SIZE:
                    self._flush_inventory(batch, source_root)
                    batch = []
            except Exception as e:
                logger.error(f"Error stat-ing {path_obj}: {e}")

        if batch:
            self._flush_inventory(batch, source_root)

    def _flush_inventory(self, batch: List[Dict], source_root: SourceRoot):
        """
        Upsert FileLocations with new mtime/size/last_seen.
        """
        # 1. Fetch existing
        rel_paths = [b["rel_path"] for b in batch]
        existing = self.db.execute(
            select(FileLocation.rel_path).where(
                FileLocation.source_root_id == source_root.id,
                FileLocation.rel_path.in_(rel_paths)
            )
        ).fetchall()
        existing_paths = {row[0] for row in existing}

        # 2. Insert New
        new_records = []
        update_records = []
        
        for item in batch:
            if item["rel_path"] not in existing_paths:
                new_records.append({
                    "source_root_id": source_root.id,
                    "rel_path": item["rel_path"],
                    "filename": item["filename"],
                    "last_known_mtime": item["mtime"],
                    "last_known_size": item["size"],
                    "last_seen_time": datetime.now()
                })
            else:
                update_records.append({
                    "rel_path": item["rel_path"],
                    "mtime": item["mtime"],
                    "size": item["size"],
                    "now": datetime.now()
                })

        if new_records:
            self.db.execute(insert(FileLocation), new_records)
        
        if update_records:
            # Simple executemany update for SQLite compatibility/speed trade-off
            self.db.execute(
                update(FileLocation)
                .where(
                    FileLocation.source_root_id == source_root.id,
                    FileLocation.rel_path == text(":rel_path")
                )
                .values(
                    last_known_mtime=text(":mtime"),
                    last_known_size=text(":size"),
                    last_seen_time=text(":now")
                ),
                update_records
            )
            
        self.db.commit()


class TaggerService:
    """
    Component 2: The Tagger.
    Logic-heavy component. Polls DB for "dirty" files.
    Matches RoutingRules -> Hashes Content -> Creates Versions -> Queues Jobs.
    """
    def __init__(self, db: Session):
        self.db = db
        self.WORKERS = 4

    def run(self, source_root: SourceRoot):
        logger.info(f"Tagger running for {source_root.path}")
        
        # 1. Load Rules
        rules = self.db.query(RoutingRule).order_by(RoutingRule.priority.desc()).all()
        plugins = self.db.query(PluginConfig).all()
        
        # 2. Find "Dirty" Files
        candidates = self.db.execute(
            select(FileLocation, FileVersion)
            .outerjoin(FileVersion, FileLocation.current_version_id == FileVersion.id)
            .where(FileLocation.source_root_id == source_root.id)
        ).all()
        
        dirty_items = []
        for loc, ver in candidates:
            needs_processing = False
            
            if not ver:
                needs_processing = True
            elif loc.last_known_mtime:
                ver_ts = ver.modified_time.timestamp()
                if abs(loc.last_known_mtime - ver_ts) > 0.1:
                    needs_processing = True
            
            if needs_processing:
                path_obj = PurePath(loc.rel_path)
                matched_tags = set()
                for r in rules:
                    if path_obj.match(r.pattern):
                        matched_tags.add(r.tag)
                
                if matched_tags:
                    dirty_items.append((loc, matched_tags))

        if not dirty_items:
            return

        logger.info(f"Tagger found {len(dirty_items)} dirty files to process.")
        
        root_path = Path(source_root.path)
        
        with ThreadPoolExecutor(max_workers=self.WORKERS) as executor:
            futures = {}
            for loc, tags in dirty_items:
                full_path = root_path / loc.rel_path
                futures[executor.submit(calculate_hash_and_stat, full_path)] = (loc, tags)
            
            for f in as_completed(futures):
                loc, tags = futures[f]
                result = f.result()
                
                if result:
                    f_hash, f_size = result
                    self._promote_version(loc, f_hash, f_size, tags, plugins)

    def _promote_version(self, loc: FileLocation, f_hash: str, f_size: int, tags: Set[str], plugins: List[PluginConfig]):
        """Creates HashRegistry, FileVersion, and Jobs."""
        
        if not self.db.get(FileHashRegistry, f_hash):
            try:
                self.db.add(FileHashRegistry(content_hash=f_hash, size_bytes=f_size))
                self.db.commit()
            except Exception:
                self.db.rollback() 

        tag_str = ",".join(sorted(list(tags)))
        
        new_ver = FileVersion(
            location_id=loc.id,
            content_hash=f_hash,
            size_bytes=f_size,
            modified_time=datetime.fromtimestamp(loc.last_known_mtime or time.time()),
            applied_tags=tag_str
        )
        self.db.add(new_ver)
        self.db.commit()
        
        loc.current_version_id = new_ver.id
        self.db.add(loc)
        
        for plugin in plugins:
            if not plugin.subscription_tags: continue
            
            sub_tags = set([t.strip() for t in plugin.subscription_tags.split(",")])
            if tags.intersection(sub_tags):
                priority = calculate_priority_from_mtime(loc.last_known_mtime or time.time())
                job = ProcessingJob(
                    file_version_id=new_ver.id,
                    plugin_name=plugin.plugin_name,
                    status=StatusEnum.QUEUED,
                    priority=priority
                )
                self.db.add(job)
        
        self.db.commit()


class Scout:
    """
    Facade for the decoupled Scout architecture.
    Maintains API compatibility with existing tests.
    """
    def __init__(self, db: Session):
        self.scanner = InventoryScanner(db)
        self.tagger = TaggerService(db)

    def scan_source(self, source_root: SourceRoot):
        # 1. Update Inventory (Fast I/O)
        self.scanner.scan(source_root)
        
        # 2. Process Logic (DB -> Hash -> Job)
        self.tagger.run(source_root)