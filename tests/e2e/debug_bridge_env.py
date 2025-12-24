#!/usr/bin/env python3
"""
Diagnostic script to isolate pyarrow import failure in bridge subprocess.

Tests multiple hypotheses:
1. Environment inheritance issue
2. VIRTUAL_ENV setting
3. Python path resolution with symlinks
4. spawn_blocking thread context differences
"""

import subprocess
import os
import sys
import base64
from pathlib import Path

# Paths
VENV_SYMLINK = Path.home() / ".casparian_flow/venvs/test_env_hash_123"
REAL_VENV = Path("/Users/shan/workspace/casparianflow/.venv")
PYTHON_VIA_SYMLINK = VENV_SYMLINK / "bin/python"
PYTHON_VIA_REAL = REAL_VENV / "bin/python"
BRIDGE_SHIM = Path("/Users/shan/workspace/casparianflow/src/casparian_flow/engine/bridge_shim.py")

# Simple test code
SIMPLE_IMPORT = "import pyarrow; print('OK:', pyarrow.__version__)"
PLUGIN_CODE = '''import pyarrow as pa
class Handler:
    def execute(self, file_path):
        yield None
'''

def run_test(name: str, python_path: Path, env: dict, code: str) -> tuple[bool, str]:
    """Run a test and return (success, output)."""
    try:
        result = subprocess.run(
            [str(python_path), "-c", code],
            env=env,
            capture_output=True,
            text=True,
            timeout=10
        )
        success = result.returncode == 0
        output = result.stdout if success else result.stderr[:500]
        return success, output
    except Exception as e:
        return False, str(e)

def run_bridge_test(name: str, python_path: Path, env: dict) -> tuple[bool, str]:
    """Run bridge_shim.py with plugin code."""
    env = env.copy()
    env['BRIDGE_SOCKET'] = '/tmp/nonexistent.sock'
    env['BRIDGE_PLUGIN_CODE'] = base64.b64encode(PLUGIN_CODE.encode()).decode()
    env['BRIDGE_FILE_PATH'] = '/tmp/test.csv'
    env['BRIDGE_JOB_ID'] = '1'
    env['BRIDGE_FILE_VERSION_ID'] = '1'

    try:
        result = subprocess.run(
            [str(python_path), str(BRIDGE_SHIM)],
            env=env,
            capture_output=True,
            text=True,
            timeout=10
        )
        # Bridge will fail on socket, but should get past pyarrow import
        if "cannot import" in result.stderr:
            return False, result.stderr[:500]
        elif "No such file or directory" in result.stderr:
            return True, "Passed pyarrow import (failed on socket as expected)"
        else:
            return result.returncode == 0, result.stdout + result.stderr[:200]
    except Exception as e:
        return False, str(e)

def main():
    print("=" * 60)
    print("Bridge Environment Diagnostic")
    print("=" * 60)

    # Verify paths exist
    print(f"\nPaths:")
    print(f"  Symlink venv: {VENV_SYMLINK} -> {VENV_SYMLINK.resolve() if VENV_SYMLINK.exists() else 'NOT FOUND'}")
    print(f"  Real venv: {REAL_VENV} {'EXISTS' if REAL_VENV.exists() else 'NOT FOUND'}")
    print(f"  Python via symlink: {PYTHON_VIA_SYMLINK} {'EXISTS' if PYTHON_VIA_SYMLINK.exists() else 'NOT FOUND'}")

    # Test configurations
    tests = []

    # Base environments
    minimal_env = {
        'PATH': os.environ.get('PATH', ''),
        'HOME': os.environ.get('HOME', ''),
    }

    full_env = os.environ.copy()

    # Test 1: Minimal env, symlink python, no VIRTUAL_ENV
    tests.append(("1. Minimal env, symlink python, no VIRTUAL_ENV",
                  PYTHON_VIA_SYMLINK, minimal_env.copy(), SIMPLE_IMPORT))

    # Test 2: Minimal env, symlink python, VIRTUAL_ENV=real
    env2 = minimal_env.copy()
    env2['VIRTUAL_ENV'] = str(REAL_VENV)
    tests.append(("2. Minimal env, symlink python, VIRTUAL_ENV=real",
                  PYTHON_VIA_SYMLINK, env2, SIMPLE_IMPORT))

    # Test 3: Minimal env, real python, no VIRTUAL_ENV
    tests.append(("3. Minimal env, real python, no VIRTUAL_ENV",
                  PYTHON_VIA_REAL, minimal_env.copy(), SIMPLE_IMPORT))

    # Test 4: Full env, symlink python, no VIRTUAL_ENV
    tests.append(("4. Full env, symlink python, no VIRTUAL_ENV",
                  PYTHON_VIA_SYMLINK, full_env.copy(), SIMPLE_IMPORT))

    # Test 5: Full env, symlink python, VIRTUAL_ENV=real
    env5 = full_env.copy()
    env5['VIRTUAL_ENV'] = str(REAL_VENV)
    tests.append(("5. Full env, symlink python, VIRTUAL_ENV=real",
                  PYTHON_VIA_SYMLINK, env5, SIMPLE_IMPORT))

    # Test 6: Full env, real python, VIRTUAL_ENV=real
    env6 = full_env.copy()
    env6['VIRTUAL_ENV'] = str(REAL_VENV)
    tests.append(("6. Full env, real python, VIRTUAL_ENV=real",
                  PYTHON_VIA_REAL, env6, SIMPLE_IMPORT))

    print("\n" + "=" * 60)
    print("Simple Import Tests (python -c 'import pyarrow')")
    print("=" * 60)

    for name, python, env, code in tests:
        success, output = run_test(name, python, env, code)
        status = "✓ PASS" if success else "✗ FAIL"
        print(f"\n{status}: {name}")
        if not success:
            print(f"    Error: {output[:200]}")

    print("\n" + "=" * 60)
    print("Bridge Shim Tests (with plugin code exec)")
    print("=" * 60)

    bridge_tests = [
        ("B1. Minimal env, symlink python, VIRTUAL_ENV=real",
         PYTHON_VIA_SYMLINK, {**minimal_env, 'VIRTUAL_ENV': str(REAL_VENV)}),
        ("B2. Full env, symlink python, VIRTUAL_ENV=real",
         PYTHON_VIA_SYMLINK, {**full_env, 'VIRTUAL_ENV': str(REAL_VENV)}),
        ("B3. Full env, real python, VIRTUAL_ENV=real",
         PYTHON_VIA_REAL, {**full_env, 'VIRTUAL_ENV': str(REAL_VENV)}),
    ]

    for name, python, env in bridge_tests:
        success, output = run_bridge_test(name, python, env)
        status = "✓ PASS" if success else "✗ FAIL"
        print(f"\n{status}: {name}")
        print(f"    Output: {output[:300]}")

    print("\n" + "=" * 60)
    print("Environment Analysis")
    print("=" * 60)

    # Check for potentially problematic vars
    problem_vars = ['PYTHONPATH', 'PYTHONHOME', '__PYVENV_LAUNCHER__',
                    'CONDA_PREFIX', 'VIRTUAL_ENV']
    print("\nPotentially problematic environment variables:")
    for var in problem_vars:
        val = os.environ.get(var)
        if val:
            print(f"  {var}={val}")

    print("\nDone.")

if __name__ == "__main__":
    main()
