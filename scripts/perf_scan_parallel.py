#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed


def run_scan(binary, path, backend):
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


def main():
    parser = argparse.ArgumentParser(description="Parallel scan perf baseline")
    parser.add_argument("path", help="Path to scan (e.g., /Users/shan)")
    parser.add_argument("--runs", type=int, default=2, help="Total runs to execute")
    parser.add_argument("--parallel", type=int, default=2, help="Parallel workers")
    parser.add_argument("--backend", choices=["duckdb"], default=None, help="Force DB backend")
    parser.add_argument("--binary", default="./target/release/casparian", help="Casparian binary")
    args = parser.parse_args()

    if not os.path.exists(args.binary):
        raise SystemExit(f"Binary not found: {args.binary}")

    runs = max(1, args.runs)
    parallel = max(1, args.parallel)

    print(f"Scan target: {args.path}")
    print(f"Runs: {runs} | Parallel: {parallel} | Backend: {args.backend or 'default'}")
    print("")

    results = []
    with ThreadPoolExecutor(max_workers=parallel) as pool:
        futures = [
            pool.submit(run_scan, args.binary, args.path, args.backend)
            for _ in range(runs)
        ]
        for idx, future in enumerate(as_completed(futures), 1):
            result = future.result()
            result["run"] = idx
            results.append(result)

    results.sort(key=lambda r: r["run"])
    ok_results = [r for r in results if r["ok"]]

    print("run  seconds  files   dirs    files/s    MB/s")
    for r in results:
        if not r["ok"]:
            print(f"{r['run']:>3}  {r['elapsed_s']:>7.2f}  ERROR: {r['stderr']}")
            continue
        files = r["total_files"]
        dirs = r["directories_scanned"]
        secs = r["elapsed_s"]
        mb = r["total_size"] / (1024 * 1024)
        files_s = files / secs if secs > 0 else 0
        mb_s = mb / secs if secs > 0 else 0
        print(f"{r['run']:>3}  {secs:>7.2f}  {files:>6}  {dirs:>6}  {files_s:>8.1f}  {mb_s:>7.2f}")

    if ok_results:
        avg_secs = sum(r["elapsed_s"] for r in ok_results) / len(ok_results)
        avg_files = sum(r["total_files"] for r in ok_results) / len(ok_results)
        avg_dirs = sum(r["directories_scanned"] for r in ok_results) / len(ok_results)
        avg_mb = sum(r["total_size"] for r in ok_results) / len(ok_results) / (1024 * 1024)
        avg_files_s = avg_files / avg_secs if avg_secs > 0 else 0
        avg_mb_s = avg_mb / avg_secs if avg_secs > 0 else 0
        print("")
        print(f"avg {avg_secs:>7.2f}  {avg_files:>6.0f}  {avg_dirs:>6.0f}  {avg_files_s:>8.1f}  {avg_mb_s:>7.2f}")


if __name__ == "__main__":
    main()
