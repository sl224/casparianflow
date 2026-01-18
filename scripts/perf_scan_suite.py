#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import tempfile
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

        with tqdm(total=1, desc="Baseline scan", unit="dir") as pbar:
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


def run_casparian_scan(binary, path, backend):
    casparian_home = tempfile.mkdtemp(prefix="casparian_perf_scan_")
    env = os.environ.copy()
    env["CASPARIAN_HOME"] = casparian_home
    if backend:
        env["CASPARIAN_DB_BACKEND"] = backend

    start = time.perf_counter()
    result = subprocess.run(
        [binary, "scan", path, "--json"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    end = time.perf_counter()

    if result.returncode != 0:
        return {
            "ok": False,
            "elapsed_s": end - start,
            "stderr": result.stderr.strip(),
        }

    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        return {
            "ok": False,
            "elapsed_s": end - start,
            "stderr": f"JSON parse error: {exc}",
        }

    summary = payload.get("summary", {})
    return {
        "ok": True,
        "elapsed_s": end - start,
        "total_files": summary.get("total_files", 0),
        "total_size": summary.get("total_size", 0),
        "directories_scanned": summary.get("directories_scanned", 0),
    }


def print_result(label, elapsed_s, files, dirs, bytes_total):
    mb = bytes_total / (1024 * 1024)
    files_s = files / elapsed_s if elapsed_s > 0 else 0
    mb_s = mb / elapsed_s if elapsed_s > 0 else 0
    print(f"{label:12} {elapsed_s:>8.2f}s  files={files:<8} dirs={dirs:<7} files/s={files_s:>8.1f}  MB/s={mb_s:>7.2f}")


def main():
    parser = argparse.ArgumentParser(description="Scan perf suite: baseline + Casparian.")
    parser.add_argument("path", help="Root path to scan (e.g., /Users/shan)")
    parser.add_argument("--workers", type=int, default=256, help="Baseline ThreadPoolExecutor workers")
    parser.add_argument("--backend", action="append", choices=["duckdb"], help="Casparian backends to test")
    parser.add_argument("--binary", default="./target/release/casparian", help="Casparian binary")
    args = parser.parse_args()

    if not os.path.exists(args.binary):
        raise SystemExit(f"Binary not found: {args.binary}")

    backends = args.backend or ["duckdb"]

    print(f"Scan target: {args.path}")
    print("")

    start = time.perf_counter()
    dirs_scanned, files_found, bytes_total = parallel_scan(args.path, args.workers)
    end = time.perf_counter()
    print_result("baseline", end - start, files_found, dirs_scanned, bytes_total)
    print("")

    for backend in backends:
        result = run_casparian_scan(args.binary, args.path, backend)
        if not result["ok"]:
            print(f"{backend:12} ERROR after {result['elapsed_s']:.2f}s: {result['stderr']}")
            continue
        print_result(
            backend,
            result["elapsed_s"],
            result["total_files"],
            result["directories_scanned"],
            result["total_size"],
        )


if __name__ == "__main__":
    main()
