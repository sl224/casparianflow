#!/usr/bin/env python3
import argparse
import os
import time
from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED

from tqdm import tqdm

IGNORE_NAMES = {
    ".Spotlight-V100",
    ".TemporaryItems",
    ".Trashes",
    ".fseventsd",
}


def should_skip_dir(path, entry_name):
    if entry_name in IGNORE_NAMES:
        return True
    if "Google Drive" in entry_name:
        return True
    if "GoogleDrive-shan@mindmadesoftware.com" in path:
        return True
    if "/Library/CloudStorage/" in path:
        return True
    if path.startswith("/Volumes/"):
        return True
    return False


def scan_dir(path):
    subdirs = []
    files = 0
    bytes_total = 0
    try:
        with os.scandir(path) as it:
            for entry in it:
                try:
                    if entry.is_dir(follow_symlinks=False):
                        if should_skip_dir(entry.path, entry.name):
                            continue
                        subdirs.append(entry.path)
                    elif entry.is_file(follow_symlinks=False):
                        files += 1
                        try:
                            bytes_total += entry.stat(follow_symlinks=False).st_size
                        except OSError:
                            pass
                except OSError:
                    continue
    except (PermissionError, OSError):
        return [], 0, 0
    return subdirs, files, bytes_total


def parallel_scan(root_path, max_workers):
    futures = {}
    dirs_scanned = 0
    files_found = 0
    bytes_total = 0

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        root_future = executor.submit(scan_dir, root_path)
        futures[root_future] = True

        with tqdm(total=1, desc="Scanning Directories", unit="dir") as pbar:
            while futures:
                done, _ = wait(futures.keys(), return_when=FIRST_COMPLETED)
                for future in done:
                    futures.pop(future, None)
                    subdirs, files, size = future.result()
                    dirs_scanned += 1
                    files_found += files
                    bytes_total += size
                    if subdirs:
                        pbar.total += len(subdirs)
                        pbar.refresh()
                    for subdir in subdirs:
                        futures[executor.submit(scan_dir, subdir)] = True
                    pbar.update(1)
                    pbar.set_postfix(files=files_found)

    return dirs_scanned, files_found, bytes_total


def main():
    parser = argparse.ArgumentParser(description="Baseline parallel filesystem scan (no DB).")
    parser.add_argument("path", help="Root path to scan")
    parser.add_argument("--workers", type=int, default=64, help="ThreadPoolExecutor workers")
    args = parser.parse_args()

    start = time.perf_counter()
    dirs_scanned, files_found, bytes_total = parallel_scan(args.path, args.workers)
    end = time.perf_counter()

    elapsed = end - start
    mb_total = bytes_total / (1024 * 1024)
    files_per_s = files_found / elapsed if elapsed > 0 else 0
    mb_per_s = mb_total / elapsed if elapsed > 0 else 0

    print(f"path: {args.path}")
    print(f"workers: {args.workers}")
    print(f"dirs: {dirs_scanned}")
    print(f"files: {files_found}")
    print(f"size_mb: {mb_total:.2f}")
    print(f"seconds: {elapsed:.2f}")
    print(f"files_per_s: {files_per_s:.1f}")
    print(f"mb_per_s: {mb_per_s:.2f}")


if __name__ == "__main__":
    main()
