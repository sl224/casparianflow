# Multi-Codex Jobs on Windows (WSL2) with Push Notifications

## Overview
Run 5-10 concurrent Codex CLI jobs on a Windows server using WSL2. Each job runs in its own git worktree and tmux session. A lightweight watcher sends push notifications when Codex needs input. The user responds by SSHing in from iPhone and attaching to the specific tmux session.

This spec targets implementation by Codex on the Windows machine. It includes all required context and deliverables.

## Goals
- Run 5-10 concurrent Codex jobs safely without file conflicts.
- Notify the user when a job needs input.
- Enable fast remote control from iPhone over SSH or RDP.

## Non-goals
- No full web UI.
- No paid notification requirement.

## Assumptions
- Windows host supports WSL2.
- Codex CLI is installed inside WSL and on PATH.
- The repo lives inside the WSL filesystem (not under /mnt/c).
- Packages can be installed in WSL as needed.

## Architecture Summary
- WSL2 hosts all Codex processes and repositories.
- Each job gets:
  - A dedicated git worktree.
  - A dedicated tmux session.
  - A dedicated log file.
- A notifier tails logs for "NEED_INPUT" and sends push notifications.

## Directory Layout
- ~/workspace/<repo>             Main repo
- ~/worktrees/<job>              Git worktree per job
- ~/.codex/jobs/<job>/            Per-job metadata
- ~/.codex/jobs/<job>/codex.log   Job log (tmux pipe-pane)
- ~/.codex/notify.env             Push provider secrets/config
- ~/bin/codex-job                 Job launcher script
- ~/bin/codex-notify              Log watcher and push sender

## Protocol for Input Requests
Codex must emit a single line when input is required:
NEED_INPUT: <short reason>

The watcher uses this line to trigger notifications.

## Implementation Steps

### 1) Install WSL2 (if needed)
From an elevated PowerShell:
```
wsl --install -d Ubuntu
wsl --set-default-version 2
```

### 2) Install dependencies in WSL
```
sudo apt update
sudo apt install -y git tmux python3 curl ripgrep
```

### 3) Create directories
```
mkdir -p ~/worktrees ~/.codex/jobs ~/bin
```
Add to PATH in ~/.bashrc or ~/.zshrc:
```
export PATH="$HOME/bin:$PATH"
```

### 4) Implement ~/bin/codex-job
Create the script at ~/bin/codex-job and chmod +x.

Responsibilities:
- new <job> [base-branch]
  - Create worktree in ~/worktrees/<job>
  - Create branch job/<job> from base-branch (default main)
  - Create ~/.codex/jobs/<job>/
- start <job> [--auto]
  - Create tmux session "cx-<job>" in worktree
  - Pipe tmux output to ~/.codex/jobs/<job>/codex.log
  - Optionally auto-run "codex" inside the session
- attach <job>, stop <job>, list, logs <job>

Suggested implementation (bash):
```
#!/usr/bin/env bash
set -euo pipefail

ROOT="$HOME/workspace/$(basename "$(git rev-parse --show-toplevel)")"
JOBS_DIR="$HOME/.codex/jobs"
WORKTREES="$HOME/worktrees"

cmd="${1:-}"
job="${2:-}"

usage() {
  echo "Usage: codex-job {new|start|attach|stop|list|logs} <job> [args]"
}

ensure_repo() {
  git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null
}

case "$cmd" in
  new)
    ensure_repo
    base="${3:-main}"
    mkdir -p "$WORKTREES" "$JOBS_DIR/$job"
    git -C "$ROOT" worktree add "$WORKTREES/$job" -b "job/$job" "$base"
    ;;
  start)
    ensure_repo
    log="$JOBS_DIR/$job/codex.log"
    session="cx-$job"
    if tmux has-session -t "$session" 2>/dev/null; then
      tmux attach -t "$session"
      exit 0
    fi
    tmux new-session -d -s "$session" -c "$WORKTREES/$job"
    tmux pipe-pane -o -t "$session" "stdbuf -oL cat >> '$log'"
    if [[ "${3:-}" == "--auto" ]]; then
      tmux send-keys -t "$session" "codex" Enter
    fi
    tmux attach -t "$session"
    ;;
  attach)
    tmux attach -t "cx-$job"
    ;;
  stop)
    tmux kill-session -t "cx-$job"
    ;;
  list)
    tmux ls || true
    ;;
  logs)
    tail -f "$JOBS_DIR/$job/codex.log"
    ;;
  *)
    usage
    exit 1
    ;;
esac
```

### 5) Implement ~/bin/codex-notify
Create the script at ~/bin/codex-notify and chmod +x.

Behavior:
- Reads ~/.codex/notify.env
- Supports PROVIDER=telegram or PROVIDER=ntfy
- Watches logs (glob) for "NEED_INPUT"
- Throttles notifications per log
- Persists offsets in ~/.codex/notify.state.json

Key fields in notify.env:
- PROVIDER=telegram or ntfy
- THROTTLE_SECONDS=600
- LOG_GLOB=/home/<user>/.codex/jobs/*/codex.log

Telegram fields:
- BOT_TOKEN=123:ABC
- CHAT_ID=123456789

ntfy fields:
- NTFY_SERVER=https://ntfy.sh
- NTFY_TOPIC=codex-<random>

Suggested implementation (python3):
```
#!/usr/bin/env python3
import argparse
import glob
import json
import os
import re
import time
import urllib.parse
import urllib.request

ENV_PATH = os.path.expanduser("~/.codex/notify.env")
STATE_PATH = os.path.expanduser("~/.codex/notify.state.json")
ANSI_RE = re.compile(r"\x1b\\[[0-9;]*[a-zA-Z]")

def load_env():
    env = {}
    if os.path.exists(ENV_PATH):
        with open(ENV_PATH, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                k, v = line.split("=", 1)
                env[k.strip()] = v.strip()
    return env

def save_state(state):
    with open(STATE_PATH, "w", encoding="utf-8") as f:
        json.dump(state, f)

def load_state():
    if os.path.exists(STATE_PATH):
        with open(STATE_PATH, "r", encoding="utf-8") as f:
            return json.load(f)
    return {}

def notify_telegram(env, message):
    token = env["BOT_TOKEN"]
    chat_id = env["CHAT_ID"]
    url = f"https://api.telegram.org/bot{token}/sendMessage"
    data = urllib.parse.urlencode({"chat_id": chat_id, "text": message}).encode()
    urllib.request.urlopen(url, data=data, timeout=10).read()

def notify_ntfy(env, message):
    server = env.get("NTFY_SERVER", "https://ntfy.sh")
    topic = env["NTFY_TOPIC"]
    url = f"{server}/{topic}"
    req = urllib.request.Request(url, data=message.encode("utf-8"), method="POST")
    req.add_header("Title", "Codex NEED_INPUT")
    urllib.request.urlopen(req, timeout=10).read()

def send_notification(env, message):
    provider = env.get("PROVIDER", "ntfy")
    if provider == "telegram":
        notify_telegram(env, message)
    elif provider == "ntfy":
        notify_ntfy(env, message)
    else:
        raise SystemExit(f"Unsupported PROVIDER: {provider}")

def scan_once(env, state, verbose=False):
    logs = glob.glob(env.get("LOG_GLOB", os.path.expanduser("~/.codex/jobs/*/codex.log")))
    throttle = int(env.get("THROTTLE_SECONDS", "600"))
    now = time.time()

    for path in logs:
        entry = state.get(path, {"offset": 0, "last": 0})
        try:
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                f.seek(entry["offset"])
                for line in f:
                    clean = ANSI_RE.sub("", line).strip()
                    if "NEED_INPUT" in clean and (now - entry["last"] > throttle):
                        message = f"NEED_INPUT: {os.path.basename(os.path.dirname(path))}"
                        if verbose:
                            print(f"Notify: {message}")
                        send_notification(env, message)
                        entry["last"] = now
                entry["offset"] = f.tell()
        except FileNotFoundError:
            continue
        state[path] = entry
    return state

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--test", type=str)
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    env = load_env()
    if args.test:
        send_notification(env, args.test)
        return

    state = load_state()
    while True:
        state = scan_once(env, state, verbose=args.verbose)
        save_state(state)
        if args.once:
            break
        time.sleep(2)

if __name__ == "__main__":
    main()
```

### 6) notify.env templates
Telegram:
```
PROVIDER=telegram
BOT_TOKEN=123:ABC
CHAT_ID=123456789
THROTTLE_SECONDS=600
LOG_GLOB=/home/<user>/.codex/jobs/*/codex.log
```

ntfy:
```
PROVIDER=ntfy
NTFY_SERVER=https://ntfy.sh
NTFY_TOPIC=codex-<random-long-string>
THROTTLE_SECONDS=600
LOG_GLOB=/home/<user>/.codex/jobs/*/codex.log
```

### 7) Run notifier
Start in its own tmux session:
```
tmux new -s codex-notify 'codex-notify'
```

Optional: if systemd in WSL is enabled, create a user service.

## Remote Access from iPhone
- Preferred: Tailscale on Windows, SSH to Windows, then `wsl` into Linux.
- Alternative: Tailscale inside WSL and SSH directly to WSL.
- RDP is OK if you prefer a GUI; SSH is faster for tmux.

## Workflow Example
```
codex-job new feature-foo
codex-job start feature-foo --auto
# In Codex: "When you need my input, output NEED_INPUT: <reason> and wait."
```

Detach and wait. When notified, SSH in and attach:
```
codex-job attach feature-foo
```

## Acceptance Criteria
- 5-10 jobs run concurrently without file conflicts.
- Each job logs to ~/.codex/jobs/<job>/codex.log.
- Notifications fire within ~10 seconds after NEED_INPUT appears.
- User can respond from iPhone via SSH and tmux.
