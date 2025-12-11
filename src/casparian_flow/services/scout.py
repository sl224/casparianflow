# src/casparian_flow/services/scout.py
import hashlib
import logging
import time
from pathlib import Path, PurePath
from datetime import datetime, timedelta
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import List, Dict, Tuple, Set, Optional

from sqlalchemy.orm import Session
from sqlalchemy import select, insert, text
from casparian_flow.db.models import (
    FileLocation, FileVersion, FileHashRegistry, ProcessingJob, 
    SourceRoot, StatusEnum, RoutingRule, PluginConfig
)
from casparian_flow.services.fs_engine import ParallelFileScanner

logger = logging.getLogger(__name__)

# Configurable stability delay for detecting files in transit
STABILITY_DELAY_SECONDS = 0.1


def calculate_hash_and_stat(filepath: Path) -> Optional[Tuple[Path, str, int, float, str]]:
    """
    Worker function. Returns (original_path_obj, hash, size, mtime, filename_str).
    Returns None if file cannot be read or is still being written.
    
    CRITICAL FIX: Implements stability check to detect files in transit.
    If a file's mtime or size changes between two checks, it's being written.
    """
    try:
        # Stability check: compare stat twice to detect in-transit files
        stat1 = filepath.stat()
        time.sleep(STABILITY_DELAY_SECONDS)
        stat2 = filepath.stat()
        
        if stat1.st_mtime != stat2.st_mtime or stat1.st_size != stat2.st_size:
            logger.debug(f"Skipping in-transit file: {filepath}")
            return None
        
        # File is stable, proceed with hashing
        hasher = hashlib.sha256()
        # Use a larger buffer (64KB) for faster network reads
        with open(filepath, "rb") as f:
            while chunk := f.read(65536): 
                hasher.update(chunk)
        return (filepath, hasher.hexdigest(), stat2.st_size, stat2.st_mtime, filepath.name)
    except Exception as e:
        logger.warning(f"Skipping {filepath}: {e}")
        return None

def calculate_priority_from_mtime(mtime: float) -> int:
    """
    QoS Priority Assignment based on file recency.
    
    Priority Tiers:
    - 100: Real-time (files modified < 24 hours ago)
    - 50:  Recent (files modified < 7 days ago) 
    - 10:  Historical backlog (older files)
    
    Higher priority jobs are processed first by the Worker.
    """
    now = datetime.now()
    file_dt = datetime.fromtimestamp(mtime)
    age = now - file_dt
    
    if age < timedelta(days=1):
        return 100  # Real-time / SLA-critical
    elif age < timedelta(days=7):
        return 50   # Recent
    else:
        return 10   # Historical backlog


class Scout:
    def __init__(self, db: Session):
        self.db = db
        self.scanner = ParallelFileScanner()
        self.BATCH_SIZE = 2000
        self.HASH_WORKERS = 8 

    def scan_source(self, source_root: SourceRoot):
        root_path = Path(source_root.path)
        if not root_path.exists():
            logger.error(f"Source root not found: {root_path}")
            return

        logger.info(f"Scouting {root_path} [Batch Mode]...")
        
        # 1. Pre-load Rules (Small dataset, keep in memory)
        self.rules = self.db.query(RoutingRule).order_by(RoutingRule.priority.desc()).all()
        self.plugins = self.db.query(PluginConfig).all()

        # 2. Define Filter
        def filter_file(entry):
            return not entry.name.startswith(".")

        # 3. Execution Pipeline
        # Scanner yields paths -> ThreadPool hashes them -> Main thread batches DB writes
        with ThreadPoolExecutor(max_workers=self.HASH_WORKERS) as executor:
            # Submit all tasks. 
            # Note: For millions of files, this holds millions of Future objects in memory.
            # Practical tradeoff: 1M futures is ~100MB RAM. Acceptable for this appliance.
            futures = {
                executor.submit(calculate_hash_and_stat, p): p 
                for p in self.scanner.walk(root_path, filter_file)
            }
            
            current_batch = []
            
            # Process results as they finish (Order Independent)
            for f in as_completed(futures):
                result = f.result()
                if result:
                    path_obj, f_hash, f_size, f_mtime, f_name = result
                    
                    # Calculate relative path for DB key
                    try:
                        rel_path = str(path_obj.relative_to(root_path))
                        current_batch.append({
                            "rel_path": rel_path,
                            "hash": f_hash,
                            "size": f_size,
                            "mtime": f_mtime,
                            "filename": f_name
                        })
                    except ValueError:
                        logger.error(f"Path issue: {path_obj}")

                # Flush if full
                if len(current_batch) >= self.BATCH_SIZE:
                    self._flush_batch(current_batch, source_root)
                    current_batch = []
            
            # Final Flush
            if current_batch:
                self._flush_batch(current_batch, source_root)

    def _flush_batch(self, batch: List[Dict], source_root: SourceRoot):
        """
        Efficiently writes a batch of files to DB using Set-based logic (Bulk Operations).
        """
        if not batch: return
        logger.info(f"Flushing batch of {len(batch)} files...")
        
        # --- Step A: Register New Hashes ---
        seen_hashes = set(item['hash'] for item in batch)
        
        # Find which hashes already exist
        existing_hashes_q = self.db.execute(
            select(FileHashRegistry.content_hash)
            .where(FileHashRegistry.content_hash.in_(seen_hashes))
        ).fetchall()
        existing_hashes = {row[0] for row in existing_hashes_q}
        
        # Insert only new ones
        new_hashes_data = []
        unique_batch_hashes = set()
        for item in batch:
            h = item['hash']
            if h not in existing_hashes and h not in unique_batch_hashes:
                new_hashes_data.append({
                    "content_hash": h,
                    "size_bytes": item['size']
                })
                unique_batch_hashes.add(h)
        
        if new_hashes_data:
            self.db.execute(insert(FileHashRegistry), new_hashes_data)

        # --- Step B: Ensure FileLocations Exist ---
        rel_paths = [item['rel_path'] for item in batch]
        
        # Map existing paths to IDs
        existing_locs_q = self.db.execute(
            select(FileLocation.rel_path, FileLocation.id, FileLocation.current_version_id)
            .where(FileLocation.source_root_id == source_root.id)
            .where(FileLocation.rel_path.in_(rel_paths))
        ).fetchall()
        
        # Map: rel_path -> (loc_id, current_ver_id)
        loc_map = {row.rel_path: (row.id, row.current_version_id) for row in existing_locs_q}
        
        new_locs_data = []
        for item in batch:
            if item['rel_path'] not in loc_map:
                new_locs_data.append({
                    "source_root_id": source_root.id,
                    "rel_path": item['rel_path'],
                    "filename": item['filename']
                })
        
        if new_locs_data:
            self.db.execute(insert(FileLocation), new_locs_data)
            self.db.commit() # Commit to generate IDs
            
            # Re-fetch to get the new IDs (Reliable cross-db approach)
            # Optimization: Only fetch the ones we just inserted
            added_paths = [x['rel_path'] for x in new_locs_data]
            refetch_q = self.db.execute(
                select(FileLocation.rel_path, FileLocation.id, FileLocation.current_version_id)
                .where(FileLocation.source_root_id == source_root.id)
                .where(FileLocation.rel_path.in_(added_paths))
            ).fetchall()
            for row in refetch_q:
                loc_map[row.rel_path] = (row.id, row.current_version_id)

        # --- Step C: Detect Version Drift ---
        # Get hashes of current versions to compare
        active_version_ids = [v[1] for v in loc_map.values() if v[1] is not None]
        active_hashes = {}
        
        if active_version_ids:
            # Chunk this if active_version_ids is massive (sqlite limit), but 2000 is safe
            av_q = self.db.execute(
                select(FileVersion.id, FileVersion.content_hash)
                .where(FileVersion.id.in_(active_version_ids))
            ).fetchall()
            active_hashes = {row.id: row.content_hash for row in av_q}

        new_versions = []
        
        for item in batch:
            loc_id, current_ver_id = loc_map.get(item['rel_path'], (None, None))
            
            # Logic: New file OR Content changed
            should_create_version = False
            if current_ver_id is None:
                should_create_version = True
            elif current_ver_id in active_hashes and active_hashes[current_ver_id] != item['hash']:
                should_create_version = True
                
            if should_create_version:
                tags = self._calculate_tags(item['rel_path'])
                new_versions.append({
                    "location_id": loc_id,
                    "content_hash": item['hash'],
                    "size_bytes": item['size'],
                    "modified_time": datetime.fromtimestamp(item['mtime']),
                    "applied_tags": tags,
                    "_mtime": item['mtime']  # Keep raw mtime for priority calc
                })

        # --- Step D: Insert Versions & Queue Jobs ---
        if new_versions:
            # Build location_id -> mtime map for priority calculation
            loc_to_mtime = {v["location_id"]: v["_mtime"] for v in new_versions}
            
            # Strip internal _mtime field before DB insert
            versions_for_db = [{k: v for k, v in ver.items() if not k.startswith("_")} 
                               for ver in new_versions]
            
            # Insert and GET IDs back
            stmt = insert(FileVersion).values(versions_for_db).returning(
                FileVersion.id, 
                FileVersion.location_id, 
                FileVersion.applied_tags
            )
            inserted_versions = self.db.execute(stmt).fetchall()
            
            # 1. Update Locations to point to new versions
            for row in inserted_versions:
                self.db.execute(
                    text("UPDATE cf_file_location SET current_version_id = :vid, last_seen_time = :now WHERE id = :lid"),
                    {"vid": row.id, "lid": row.location_id, "now": datetime.now()}
                )

            # 2. Queue Jobs with QoS Priority
            jobs_to_queue = []
            for row in inserted_versions:
                vid, loc_id, tags = row.id, row.location_id, row.applied_tags
                if not tags: continue
                
                # Calculate priority from mtime
                mtime = loc_to_mtime.get(loc_id, 0)
                priority = calculate_priority_from_mtime(mtime)
                
                file_tags = set(tags.split(","))
                for plugin in self.plugins:
                    if not plugin.subscription_tags: continue
                    sub_tags = set([t.strip() for t in plugin.subscription_tags.split(",")])
                    
                    if file_tags.intersection(sub_tags):
                        jobs_to_queue.append({
                            "file_version_id": vid,
                            "plugin_name": plugin.plugin_name,
                            "status": StatusEnum.QUEUED,
                            "priority": priority
                        })
            
            if jobs_to_queue:
                self.db.execute(insert(ProcessingJob), jobs_to_queue)

        self.db.commit()

    def _calculate_tags(self, rel_path: str) -> str:
        path_obj = PurePath(rel_path)
        tags = set()
        for rule in self.rules:
            if path_obj.match(rule.pattern):
                tags.add(rule.tag)
        return ",".join(sorted(list(tags)))