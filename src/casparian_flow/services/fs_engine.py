# src/casparian_flow/services/fs_engine.py
import logging
import os
import queue
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Callable, Iterator, Optional

logger = logging.getLogger(__name__)

class ParallelFileScanner:
    """
    Decoupled Producer-Consumer Scanner.
    
    - Producer: Walks directories using a thread pool.
    - Output: Yields Paths via a generator.
    
    This ensures that slow I/O or DB operations in the consumer (Scout)
    do not block the traversal logic.
    """

    def __init__(self, max_workers: int = 16):
        self.max_workers = max_workers
        self.queue = queue.Queue(maxsize=10000) # Backpressure limit
        self._sentinel = object()

    def walk(self, root_path: Path, filter_func: Callable[[os.DirEntry], bool]) -> Iterator[Path]:
        """
        Generator that yields paths as they are discovered.
        """
        # Start the background walker thread
        walker_thread = threading.Thread(
            target=self._run_walker, 
            args=(root_path, filter_func)
        )
        walker_thread.start()

        # Yield items from the queue until Sentinel is found
        while True:
            item = self.queue.get()
            if item is self._sentinel:
                break
            yield item
            
        walker_thread.join()

    def _run_walker(self, root: Path, filter_func: Callable):
        """
        Orchestrates the directory walk using a ThreadPool.
        """
        pending_dirs = {str(root)}
        active_futures = set()
        
        with ThreadPoolExecutor(max_workers=self.max_workers) as executor:
            # Bootstrap
            active_futures.add(executor.submit(self._scan_dir, str(root), filter_func))
            
            while active_futures:
                # Wait for any future to complete (busy wait loop with sleep is simple/robust here)
                # Ideally use as_completed, but we need to submit NEW tasks dynamically
                done_futures = []
                for f in list(active_futures):
                    if f.done():
                        done_futures.append(f)
                
                if not done_futures:
                    import time
                    time.sleep(0.01)
                    continue
                
                for f in done_futures:
                    active_futures.remove(f)
                    try:
                        subdirs, files = f.result()
                        
                        # 1. Enqueue Files for Consumption
                        for file_path in files:
                            self.queue.put(Path(file_path))
                            
                        # 2. Schedule Subdirectories
                        for d in subdirs:
                            active_futures.add(executor.submit(self._scan_dir, d, filter_func))
                            
                    except Exception as e:
                        logger.error(f"Scan error: {e}")

        # Signal completion
        self.queue.put(self._sentinel)

    def _scan_dir(self, path: str, filter_func: Callable):
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
            logger.debug(f"Access denied: {path}")
            
        return subdirs, files