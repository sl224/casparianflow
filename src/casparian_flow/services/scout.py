# src/casparian_flow/services/scout.py
import hashlib
import logging
import time
import pathspec
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
    IgnoreRule
)
from casparian_flow.services.fs_engine import ParallelFileScanner
from casparian_flow.services.filter_logic import PathFilter

logger = logging.getLogger(__name__)

STABILITY_DELAY_SECONDS = 0.1

def calculate_hash_and_stat(filepath: Path) -> Optional[Tuple[str, int]]:
    try:
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
    now = datetime.now()
    file_dt = datetime.fromtimestamp(mtime)
    age = now - file_dt
    if age < timedelta(days=1): return 100
    elif age < timedelta(days=7): return 50
    else: return 10

class InventoryScanner:
    def __init__(self, db: Session):
        self.db = db
        self.scanner = ParallelFileScanner()
        self.BATCH_SIZE = 2000

    def scan(self, source_root: SourceRoot):
        root_path = Path(source_root.path)
        if not root_path.exists(): return

        logger.info(f"Inventory Scan: {root_path}")

        # Load Ignore Rules
        ignore_rules = self.db.query(IgnoreRule.pattern).filter(
            (IgnoreRule.source_root_id == source_root.id) | (IgnoreRule.source_root_id == None),
            IgnoreRule.active == True
        ).all()
        patterns = [r[0] for r in ignore_rules]
        path_filter = PathFilter(patterns)

        def filter_func(entry):
            if entry.name.startswith("."): return False
            try:
                rel_p = str(Path(entry.path).relative_to(root_path))
                if path_filter.is_ignored(rel_p): return False
            except Exception: pass
            return True

        batch = []
        for path_obj in self.scanner.walk(root_path, filter_func):
            try:
                # Double check path filter on relative path
                rel_path = str(path_obj.relative_to(root_path))
                if path_filter.is_ignored(rel_path): continue

                stat = path_obj.stat()
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
                logger.error(f"Error scanning {path_obj}: {e}")

        if batch:
            self._flush_inventory(batch, source_root)

    def _flush_inventory(self, batch: List[Dict], source_root: SourceRoot):
        rel_paths = [b["rel_path"] for b in batch]

        # Fetch existing records with IDs
        existing = self.db.execute(
            select(FileLocation.id, FileLocation.rel_path).where(
                FileLocation.source_root_id == source_root.id,
                FileLocation.rel_path.in_(rel_paths)
            )
        ).fetchall()
        existing_map = {row.rel_path: row.id for row in existing}

        new_records = []
        update_records = []

        for item in batch:
            if item["rel_path"] not in existing_map:
                new_records.append({
                    "source_root_id": source_root.id,
                    "rel_path": item["rel_path"],
                    "filename": item["filename"],
                    "last_known_mtime": item["mtime"],
                    "last_known_size": item["size"],
                    "last_seen_time": datetime.now()
                })
            else:
                # Include the ID for bulk update
                update_records.append({
                    "id": existing_map[item["rel_path"]],
                    "last_known_mtime": item["mtime"],
                    "last_known_size": item["size"],
                    "last_seen_time": datetime.now()
                })

        if new_records:
            self.db.execute(insert(FileLocation), new_records)

        if update_records:
            # Use ORM bulk update with primary keys
            self.db.bulk_update_mappings(FileLocation, update_records)

        self.db.commit()


class TaggerService:
    def __init__(self, db: Session):
        self.db = db
        self.WORKERS = 4

    def run(self, source_root: SourceRoot):
        logger.info(f"Tagger running for {source_root.path}")
        
        # 1. Load Rules & Plugins
        rules = self.db.query(RoutingRule).order_by(RoutingRule.priority.desc()).all()
        plugins = self.db.query(PluginConfig).all()
        
        # 2. Find Dirty Files
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
                # Calculate Tags (Pattern Matching)
                matched_tags = self._calculate_tags(loc.rel_path, rules)
                if matched_tags:
                    dirty_items.append((loc, matched_tags))

        if not dirty_items: return

        logger.info(f"Tagger found {len(dirty_items)} dirty files.")
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

    def _calculate_tags(self, rel_path: str, rules: List[RoutingRule]) -> Set[str]:
        """Match file path against Routing Rules to determine topics/tags."""
        tags = set()
        for rule in rules:
            try:
                # Use pathspec for gitignore-style glob matching
                spec = pathspec.PathSpec.from_lines("gitwildmatch", [rule.pattern])
                if spec.match_file(rel_path):
                    tags.add(rule.tag)
            except Exception:
                pass
        return tags

    def _promote_version(self, loc, f_hash, f_size, tags, plugins):
        # A. Hash Registry
        if not self.db.get(FileHashRegistry, f_hash):
            try:
                self.db.add(FileHashRegistry(content_hash=f_hash, size_bytes=f_size))
                self.db.commit()
            except Exception:
                self.db.rollback()

        tag_str = ",".join(sorted(list(tags)))
        
        # B. Create Version
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
        
        # C. Fan-Out: Queue Jobs for ALL subscribing plugins
        jobs_to_queue = []
        for plugin in plugins:
            if not plugin.subscription_tags: continue
            
            # Plugin subscriptions (Topics)
            sub_topics = set([t.strip() for t in plugin.subscription_tags.split(",")])
            
            # Intersection: Does the file's tags match the plugin's subscriptions?
            if tags.intersection(sub_topics):
                priority = calculate_priority_from_mtime(loc.last_known_mtime or time.time())
                
                logger.info(f"Routing {loc.filename} [{tags}] -> Plugin {plugin.plugin_name}")
                
                job = ProcessingJob(
                    file_version_id=new_ver.id,
                    plugin_name=plugin.plugin_name,
                    status=StatusEnum.QUEUED,
                    priority=priority
                )
                jobs_to_queue.append(job)
        
        if jobs_to_queue:
            # FIXED: Use add_all() because jobs_to_queue contains ORM objects, not dicts
            self.db.add_all(jobs_to_queue)
        
        self.db.commit()

class Scout:
    def __init__(self, db: Session):
        self.scanner = InventoryScanner(db)
        self.tagger = TaggerService(db)

    def scan_source(self, source_root: SourceRoot):
        self.scanner.scan(source_root)
        self.tagger.run(source_root)