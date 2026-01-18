#!/usr/bin/env python3
import json
import re
import sys


def extract_suggestion_lines(output):
    lines = output.splitlines()
    in_block = False
    items = []
    for line in lines:
        if "┌ SUGGESTIONS" in line:
            in_block = True
            continue
        if in_block and line.strip().startswith("└"):
            break
        if not in_block:
            continue

        # Isolate left panel text between borders to avoid right-panel bleed.
        cleaned = line
        if cleaned.startswith("│"):
            parts = cleaned.split("│")
            if len(parts) > 1:
                cleaned = parts[1]
        if cleaned.endswith("│"):
            cleaned = cleaned[:-1]
        cleaned = cleaned.strip()
        if not cleaned:
            continue
        if cleaned.startswith("Scan files to see suggestions"):
            continue
        items.append(cleaned)
    return items


def check_must_show_suffix(items):
    quality_signals = []
    example_items = []
    ellipsis_re = re.compile(r"(\.\.\.|…)")
    suffix_re = re.compile(r"(\.\.\.|…)/.{2,}")

    for item in items:
        if not ellipsis_re.search(item):
            continue
        if not suffix_re.search(item):
            quality_signals.append("suffix_hidden")
            example_items.append(item)
    return quality_signals, example_items


def check_must_differentiate(items):
    quality_signals = []
    example_items = []
    seen = {}
    for item in items:
        key = " ".join(item.split())
        seen.setdefault(key, []).append(item)
    duplicates = [vals for vals in seen.values() if len(vals) > 1]
    if duplicates:
        quality_signals.append("ambiguous_duplicates")
        for group in duplicates:
            example_items.extend(group)
    return quality_signals, example_items


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"passed": True, "quality_signals": [], "example_items": []}))
        return

    try:
        rules = json.loads(sys.argv[1])
    except json.JSONDecodeError:
        rules = []

    output = sys.stdin.read()
    items = extract_suggestion_lines(output)
    if not items:
        print(json.dumps({"passed": True, "quality_signals": [], "example_items": []}))
        return

    quality_signals = []
    example_items = []

    if "must_show_suffix" in rules:
        qs, ex = check_must_show_suffix(items)
        quality_signals.extend(qs)
        example_items.extend(ex)

    if "must_differentiate" in rules:
        qs, ex = check_must_differentiate(items)
        quality_signals.extend(qs)
        example_items.extend(ex)

    passed = len(quality_signals) == 0
    print(json.dumps({
        "passed": passed,
        "quality_signals": quality_signals,
        "example_items": example_items,
    }))


if __name__ == "__main__":
    main()
