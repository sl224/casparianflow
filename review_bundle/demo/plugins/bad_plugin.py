"""
Bad Plugin - FOR TESTING VALIDATION FAILURES

This plugin intentionally uses banned imports to test the Gatekeeper validation.
Deploying this should show validation errors in the UI.
"""

import os  # BANNED: system access
import subprocess  # BANNED: process spawning
import socket  # BANNED: network access
from pathlib import Path

TOPIC = "bad_output"
SINK = "parquet"

# This plugin should never be deployed - it's for testing the UI error display


def bad_function():
    """This would be dangerous if allowed to run."""
    os.system("echo dangerous")  # BANNED
    subprocess.run(["whoami"])   # BANNED
    socket.socket()              # BANNED


def parse(file_path: str):
    """This should never execute due to validation failures."""
    bad_function()
    return None
