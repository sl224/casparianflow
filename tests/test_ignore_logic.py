import pytest
from pathlib import Path
from casparian_flow.services.filter_logic import PathFilter

def test_defaults_ignored():
    pf = PathFilter([])
    assert pf.is_ignored(".git/config")
    assert pf.is_ignored("src/__pycache__/file.pyc")
    assert pf.is_ignored("temp_file.tmp")
    assert not pf.is_ignored("src/main.py")

def test_custom_patterns():
    pf = PathFilter(["*.log", "secrets/"])
    assert pf.is_ignored("app.log")
    assert pf.is_ignored("secrets/key.pem")
    assert not pf.is_ignored("app.txt")

def test_nested_wildcards():
    pf = PathFilter(["**/node_modules/**"])
    assert pf.is_ignored("project/node_modules/pkg/index.js")
    assert pf.is_ignored("node_modules/bin")