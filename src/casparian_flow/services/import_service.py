# src/casparian_flow/services/import_service.py
import logging
import shutil
from pathlib import Path
from datetime import datetime
from typing import List, Set, Optional

import pathspec
from sqlalchemy.orm import Session

from casparian_flow.db.models import (
    SourceRoot, FileLocation, FileVersion, FileHashRegistry,
    FileTag, ProcessingJob, StatusEnum, PluginConfig, RoutingRule
)
from casparian_flow.services.scout import (
    calculate_hash_and_stat,
    calculate_priority_from_mtime
)

logger = logging.getLogger(__name__)


class ImportService:
    """
    Handles importing new files into managed storage with full lineage tracking.

    Files are copied to a managed directory, hashed, and registered in the database.
    Both manual plugin selection and automatic tag-based routing are supported.
    """

    def __init__(self, db: Session, managed_dir: str = "data/managed"):
        """
        Initialize the import service.

        Args:
            db: SQLAlchemy database session
            managed_dir: Path to managed storage directory (default: "data/managed")
        """
        self.db = db
        self.managed_dir = Path(managed_dir).resolve()
        self.managed_root = self._ensure_managed_source_root()

    def _ensure_managed_source_root(self) -> SourceRoot:
        """Create or get the managed SourceRoot."""
        managed_path = str(self.managed_dir)

        source_root = self.db.query(SourceRoot).filter_by(
            path=managed_path
        ).first()

        if not source_root:
            # Create directory if needed
            self.managed_dir.mkdir(parents=True, exist_ok=True)

            # Create SourceRoot entry
            source_root = SourceRoot(
                path=managed_path,
                type="managed",
                active=1
            )
            self.db.add(source_root)
            self.db.commit()
            self.db.refresh(source_root)
            logger.info(f"Created managed SourceRoot: {managed_path}")

        return source_root

    def import_files(
        self,
        source_root_id: int,
        rel_paths: List[str],
        manual_tags: Set[str],
        manual_plugins: Set[str]
    ) -> List[FileLocation]:
        """
        Import files from source root to managed directory.

        Args:
            source_root_id: Source SourceRoot ID
            rel_paths: List of relative paths within source root
            manual_tags: Tags to apply manually (user-specified)
            manual_plugins: Plugins to create jobs for manually (user-selected)

        Returns:
            List of created FileLocation records

        Raises:
            ValueError: If source root not found
        """
        # 1. Validate source root
        source_root = self.db.get(SourceRoot, source_root_id)
        if not source_root:
            raise ValueError(f"SourceRoot {source_root_id} not found")

        source_path = Path(source_root.path).resolve()

        # 2. Process files
        imported = []

        for rel_path in rel_paths:
            try:
                # Security: Validate path
                src_file = (source_path / rel_path).resolve()
                if not str(src_file).startswith(str(source_path)):
                    logger.error(f"Path traversal attempt: {rel_path}")
                    continue

                if not src_file.exists() or not src_file.is_file():
                    logger.error(f"File not found or not a file: {src_file}")
                    continue

                # Import single file
                file_loc = self._import_single_file(
                    src_file=src_file,
                    manual_tags=manual_tags,
                    manual_plugins=manual_plugins
                )

                if file_loc:
                    imported.append(file_loc)

            except Exception as e:
                logger.error(f"Failed to import {rel_path}: {e}", exc_info=True)

        return imported

    def _import_single_file(
        self,
        src_file: Path,
        manual_tags: Set[str],
        manual_plugins: Set[str]
    ) -> Optional[FileLocation]:
        """
        Import a single file with full processing pipeline.

        Args:
            src_file: Source file path (already validated)
            manual_tags: User-specified tags
            manual_plugins: User-selected plugins

        Returns:
            Created FileLocation or None if failed
        """
        # 1. Generate unique destination filename
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
        dest_filename = f"{timestamp}_{src_file.name}"
        dest_path = self.managed_dir / dest_filename

        # 2. Copy file
        try:
            shutil.copy2(src_file, dest_path)
            logger.info(f"Copied {src_file} → {dest_path}")
        except Exception as e:
            logger.error(f"Failed to copy file: {e}")
            return None

        # 3. Calculate hash
        hash_result = calculate_hash_and_stat(dest_path)
        if not hash_result:
            logger.error(f"Failed to hash {dest_path}")
            dest_path.unlink()  # Cleanup
            return None

        content_hash, file_size = hash_result

        try:
            # 4. Register hash in registry
            if not self.db.get(FileHashRegistry, content_hash):
                self.db.add(FileHashRegistry(
                    content_hash=content_hash,
                    size_bytes=file_size
                ))
                self.db.flush()

            # 5. Create FileLocation
            file_location = FileLocation(
                source_root_id=self.managed_root.id,
                rel_path=dest_filename,
                filename=src_file.name,  # Keep original name for display
                last_known_mtime=dest_path.stat().st_mtime,
                last_known_size=file_size
            )
            self.db.add(file_location)
            self.db.flush()

            # 6. Calculate auto tags from routing rules
            auto_tags = self._calculate_auto_tags(src_file.name)

            # 7. Combine manual + auto tags
            all_tags = manual_tags | auto_tags
            tag_str = ",".join(sorted(list(all_tags)))

            # 8. Create FileVersion
            file_version = FileVersion(
                location_id=file_location.id,
                content_hash=content_hash,
                size_bytes=file_size,
                modified_time=datetime.fromtimestamp(dest_path.stat().st_mtime),
                applied_tags=tag_str
            )
            self.db.add(file_version)
            self.db.flush()

            # 9. Update FileLocation.current_version_id
            file_location.current_version_id = file_version.id

            # 10. Apply manual tags to FileTag table
            for tag in manual_tags:
                self.db.add(FileTag(
                    file_id=file_location.id,
                    tag=tag
                ))

            # 11. Calculate priority
            priority = calculate_priority_from_mtime(dest_path.stat().st_mtime)

            # 12. Create jobs for manually selected plugins
            for plugin_name in manual_plugins:
                self.db.add(ProcessingJob(
                    file_version_id=file_version.id,
                    plugin_name=plugin_name,
                    status=StatusEnum.QUEUED,
                    priority=priority
                ))

            # 13. ALSO apply automatic tag-based routing
            self._apply_auto_routing(
                file_version=file_version,
                tags=all_tags,
                priority=priority,
                skip_plugins=manual_plugins  # Avoid duplicate jobs
            )

            self.db.commit()

            logger.info(
                f"Imported {src_file.name} → ID {file_location.id}, "
                f"Tags: {all_tags}, Manual plugins: {manual_plugins}"
            )

            return file_location

        except Exception as e:
            self.db.rollback()
            # Cleanup copied file on database error
            if dest_path.exists():
                dest_path.unlink()
            logger.error(f"Database error during import: {e}", exc_info=True)
            return None

    def _calculate_auto_tags(self, filename: str) -> Set[str]:
        """
        Calculate automatic tags based on RoutingRules.

        Args:
            filename: Name of file to match against rules

        Returns:
            Set of matching tags
        """
        rules = self.db.query(RoutingRule).order_by(
            RoutingRule.priority.desc()
        ).all()

        tags = set()
        for rule in rules:
            try:
                spec = pathspec.PathSpec.from_lines("gitwildmatch", [rule.pattern])
                if spec.match_file(filename):
                    tags.add(rule.tag)
            except Exception as e:
                logger.error(f"Error applying rule {rule.id}: {e}")

        return tags

    def _apply_auto_routing(
        self,
        file_version: FileVersion,
        tags: Set[str],
        priority: int,
        skip_plugins: Set[str]
    ):
        """
        Apply automatic tag-based routing (like TaggerService does).

        Creates ProcessingJobs for plugins whose subscription_tags intersect with file tags.

        Args:
            file_version: The FileVersion to route
            tags: All tags (manual + auto) for this file
            priority: Job priority
            skip_plugins: Plugin names to skip (already manually selected)
        """
        plugins = self.db.query(PluginConfig).all()

        for plugin in plugins:
            # Skip if manually selected (avoid duplicates)
            if plugin.plugin_name in skip_plugins:
                continue

            if not plugin.subscription_tags:
                continue

            # Parse plugin subscriptions
            sub_topics = {t.strip() for t in plugin.subscription_tags.split(",") if t.strip()}

            # Check intersection
            if tags.intersection(sub_topics):
                logger.info(
                    f"Auto-routing to {plugin.plugin_name} based on tags {tags & sub_topics}"
                )
                self.db.add(ProcessingJob(
                    file_version_id=file_version.id,
                    plugin_name=plugin.plugin_name,
                    status=StatusEnum.QUEUED,
                    priority=priority
                ))
