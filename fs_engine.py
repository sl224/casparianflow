import logging
import os
from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
from pathlib import Path
from typing import Callable, List, TypeVar, Any, Dict
from tqdm import tqdm
import sys
import pathspec

SCAN = 0
ACTION = 1


logger = logging.getLogger(__name__)
logger.setLevel('INFO')
stdout_handler = logging.StreamHandler(sys.stdout)
logger.addHandler(stdout_handler)

T = TypeVar("T")

class ParallelFileScanner:
    """
    High-Performance Filesystem Walker.
    
    Architecture:
    - Uses a single ThreadPoolExecutor for both Traversal (IO) and Processing (CPU/IO).
    - Dynamic Work Injection: Discovering a directory immediately schedules a new scan task.
    - Backpressure: Implicitly managed by the ThreadPoolExecutor's queue.
    """

    def __init__(self, max_workers: int = 64):
        self.max_workers = max_workers

    def walk(
        self,
        root_path: Path,
        filter_func: Callable[[os.DirEntry], bool],
        action_func: Callable[[Path], T],
    ) -> List[T]:
        """
        Recursively scans root_path.
        """
        futures: Dict[Any, str] = {}
        results: List[T] = []
        
        dirs_scanned = 0
        files_found = 0

        logger.info(f"Starting parallel scan of {root_path} ({self.max_workers} workers)")

        with ThreadPoolExecutor(max_workers=self.max_workers) as executor:
            # Bootstrapping
            root_future = executor.submit(self._scan_dir, str(root_path), filter_func)
            futures[root_future] = SCAN

            # Initialize TQDM with 1 known directory (root)
            with tqdm(total=1, desc="Scanning Directories", unit="dir") as pbar:
                while futures:
                    # Wait for at least one task to finish
                    done, _ = wait(futures.keys(), return_when=FIRST_COMPLETED)

                    for f in done:
                        task_type = futures.pop(f)
                        try:
                            if task_type == SCAN:
                                dirs_scanned += 1
                                subdirs, matching_files = f.result()

                                # 1. Dynamic Total Adjustment
                                if subdirs:
                                    pbar.total += len(subdirs)
                                    pbar.refresh() # Force redraw to show correct %

                                # 2. Fan-Out: Schedule Subdirectories
                                for d in subdirs:
                                    nf = executor.submit(self._scan_dir, d, filter_func)
                                    futures[nf] = SCAN 

                                # 3. Schedule Actions
                                for p in matching_files:
                                    files_found += 1
                                    nf = executor.submit(action_func, Path(p))
                                    futures[nf] = ACTION 
                                
                                # Update Progress
                                pbar.update(1)
                                pbar.set_postfix(files=files_found)

                            elif task_type == ACTION:
                                res = f.result()
                                if res is not None:
                                    results.append(res)

                        except Exception as e:
                            logger.error(f"Task failed: {e}")
                            # Ensure progress bar doesn't stall on error
                            if task_type == SCAN:
                                pbar.update(1)

        logger.info(f"Scan complete. Scanned {dirs_scanned} dirs, processed {len(results)} files.")
        return results


    def _scan_dir(self, path: str, filter_func: Callable[[os.DirEntry], bool]):
        """
        Unit of Work: Scans a single directory.
        Returns: (list_of_subdir_paths, list_of_file_paths)
        """
        subdirs = []
        files = []

        try:
            with os.scandir(path) as it:
                for entry in it:
                    if entry.is_dir(follow_symlinks=False):
                        subdirs.append(entry.path)
                    elif entry.is_file(follow_symlinks=False):
                        if filter_func(entry):
                            files.append(entry.path)
                            
        except (PermissionError, OSError) as e:
            logger.debug(f"Access denied or error: {path} [{e}]")

        return subdirs, files

def load_pathspec(ignore_file_path):
    """
    Loads the ignore file, adds the ignore file itself to the patterns,
    and returns a compiled PathSpec object.
    """
    lines = []
    try:
        with open(ignore_file_path, "r") as f:
            lines = [str(Path(line)) for line in f.readlines()]
    except FileNotFoundError:
        logger.warning(
            f"Ignore file '{ignore_file_path}' not found. Scanning all files."
        )
    return pathspec.GitIgnoreSpec.from_lines("gitwildmatch", lines)


if __name__ == '__main__':
    s = ParallelFileScanner(2048)
    spec = load_pathspec('.scanignore')
    r = s.walk('.', 
                lambda path: not spec.match_file(path), 
                lambda path_obj: (path_obj, path_obj.stat().st_size))
    # print(len(r), ' files found')
    print(r)