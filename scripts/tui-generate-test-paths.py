#!/usr/bin/env python3
import json
import os
import re
import sys


SAFE_KEYS = {
    "Up",
    "Down",
    "Left",
    "Right",
    "Enter",
    "Tab",
    "BTab",
    "g",
    "G",
    "j",
    "k",
    "r",
    "R",
    "m",
    "f",
    "P",
    "p",
    "?",
    "Space",
}


def read_file(path):
    with open(path, "r", encoding="utf-8") as fh:
        return fh.read()


def parse_keys(raw):
    raw = raw.replace("`", "").strip()
    raw = raw.replace("Shift+Tab", "BTab")
    raw = raw.replace("PgUp", "PageUp")
    raw = raw.replace("PgDn", "PageDown")
    raw = raw.replace("Esc", "Escape")
    raw = raw.replace("↑", "Up")
    raw = raw.replace("↓", "Down")
    raw = raw.replace("←", "Left")
    raw = raw.replace("→", "Right")

    parts = re.split(r"\s*(?:/|,|\bor\b)\s*", raw)
    keys = []
    for part in parts:
        part = part.strip()
        if not part:
            continue
        range_match = re.match(r"^(\d)-(\d)$", part)
        if range_match:
            start = int(range_match.group(1))
            end = int(range_match.group(2))
            if start <= end:
                for i in range(start, end + 1):
                    keys.append(str(i))
            continue
        keys.append(part)
    return keys


def parse_nav_map(tui_spec_path):
    nav_map = {}
    for line in read_file(tui_spec_path).splitlines():
        if not line.strip().startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 2:
            continue
        key_cell = cells[0]
        action_cell = cells[1]
        match = re.search(r"Go to ([A-Za-z ]+)", action_cell)
        if not match:
            continue
        view = match.group(1).strip()
        for key in parse_keys(key_cell):
            nav_map[key] = view
    return nav_map


def parse_view_name(content):
    for line in content.splitlines():
        if line.startswith("# "):
            title = line[2:].strip()
            title = re.sub(r"\s+View.*$", "", title)
            title = re.sub(r"\s+-\s+.*$", "", title)
            return title.strip()
    return None


def parse_status(content):
    for line in content.splitlines():
        if line.strip().startswith("**Status:**"):
            return line.split("**Status:**", 1)[1].strip()
    return ""


def parse_keybindings(content):
    lines = content.splitlines()
    keys = []
    in_table = False
    for idx, line in enumerate(lines):
        if line.strip().startswith("| Key ") and "Action" in line:
            in_table = True
            continue
        if in_table:
            if not line.strip().startswith("|"):
                break
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) < 2:
                continue
            key_cell = cells[0]
            keys.extend(parse_keys(key_cell))
    return keys


def select_action_keys(view_name, nav_map, keys):
    actions = []
    for key in keys:
        if key in {"Escape", "q", "Q"}:
            continue
        if key.isdigit() and view_name != "Discover":
            nav_view = nav_map.get(key)
            if nav_view and nav_view != view_name:
                continue
        if key not in SAFE_KEYS:
            continue
        actions.append(key)
        if len(actions) >= 4:
            break
    return actions


def entry_key_for_view(view_name, nav_map, keybindings):
    if view_name == "Home":
        return []
    if view_name == "Settings":
        for key in keybindings:
            if key == ",":
                return [","]
        return [","]
    for key, view in nav_map.items():
        if view == view_name:
            return [key]
    return []


def generate_paths(specs_dir, tui_spec_path):
    nav_map = parse_nav_map(tui_spec_path)
    paths = []
    for filename in sorted(os.listdir(specs_dir)):
        if not filename.endswith(".md"):
            continue
        path = os.path.join(specs_dir, filename)
        content = read_file(path)
        status = parse_status(content).lower()
        if "obsolete" in status:
            continue
        view_name = parse_view_name(content)
        if not view_name:
            continue
        keybindings = parse_keybindings(content)
        entry_keys = entry_key_for_view(view_name, nav_map, keybindings)
        action_keys = select_action_keys(view_name, nav_map, keybindings)

        actions = [{"keys": "", "expect": view_name}]
        for key in action_keys:
            actions.append({"keys": key, "expect": ""})

        paths.append(
            {
                "id": f"{view_name.lower().replace(' ', '-')}-entry",
                "view": view_name,
                "entry": entry_keys,
                "actions": actions,
                "exit": ["Escape"] if view_name != "Home" else [],
            }
        )
    return paths


def main():
    if len(sys.argv) < 3:
        print("Usage: tui-generate-test-paths.py <specs_dir> <tui_spec>", file=sys.stderr)
        return 1
    specs_dir = sys.argv[1]
    tui_spec_path = sys.argv[2]
    paths = generate_paths(specs_dir, tui_spec_path)
    print(json.dumps(paths, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
