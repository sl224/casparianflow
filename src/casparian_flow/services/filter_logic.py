# src/casparian_flow/services/filter_logic.py
import logging
from typing import List
import pathspec

logger = logging.getLogger(__name__)


class PathFilter:
    """
    Optimized file path filtering using gitignore-style patterns.
    """

    def __init__(self, patterns: List[str]):
        # Always ignore common junk
        defaults = [".git/", "__pycache__/", "*.tmp", ".DS_Store"]

        # Combine user patterns with defaults
        # Filter empty strings/None
        valid_patterns = [p for p in patterns if p]
        all_patterns = defaults + valid_patterns

        try:
            self.spec = pathspec.PathSpec.from_lines("gitwildmatch", all_patterns)
            logger.debug(f"PathFilter initialized with {len(all_patterns)} rules")
        except Exception as e:
            logger.error(f"Failed to compile pathspec: {e}")
            # Fallback to empty spec that matches nothing
            self.spec = pathspec.PathSpec.from_lines("gitwildmatch", [])

    def is_ignored(self, rel_path: str) -> bool:
        """
        Check if the relative path matches any ignore rule.
        """
        # pathspec expects POSIX paths usually, but handles OS separators decently.
        # Ideally, ensure rel_path is relative to the root being scanned.
        try:
            return self.spec.match_file(rel_path)
        except Exception:
            return False
