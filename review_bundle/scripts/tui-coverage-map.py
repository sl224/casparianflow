#!/usr/bin/env python3
import argparse
import fcntl
import os


def load_map(fp):
    fp.seek(0)
    data = fp.read().splitlines()
    result = {}
    for line in data:
        if not line.strip():
            continue
        parts = line.split("\t", 1)
        if len(parts) != 2:
            continue
        count_str, signature = parts
        try:
            count = int(count_str)
        except ValueError:
            continue
        result[signature] = count
    return result


def write_map(fp, data):
    fp.seek(0)
    fp.truncate()
    for signature, count in data.items():
        fp.write(f"{count}\t{signature}\n")
    fp.flush()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--map", required=True)
    parser.add_argument("--signature", required=True)
    parser.add_argument("--op", choices=["get", "update"], default="get")
    args = parser.parse_args()

    os.makedirs(os.path.dirname(args.map), exist_ok=True)
    with open(args.map, "a+", encoding="utf-8") as fp:
        fcntl.flock(fp.fileno(), fcntl.LOCK_EX)
        data = load_map(fp)
        count = data.get(args.signature, 0)
        if args.op == "update":
            count += 1
            data[args.signature] = count
            write_map(fp, data)
        fcntl.flock(fp.fileno(), fcntl.LOCK_UN)

    print(count)


if __name__ == "__main__":
    main()
