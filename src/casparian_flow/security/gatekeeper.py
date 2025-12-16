# src/casparian_flow/security/gatekeeper.py
"""
Plugin Security Gatekeeper - AST-Based Validation & Signature Verification.

This module prevents malicious or dangerous code from entering the system.
All AI-generated plugins must pass these checks before deployment.

Design Principles:
- Static analysis (AST) to detect dangerous imports/calls
- HMAC signature verification for authenticity
- Content-addressable hashing for integrity
"""

import ast
import hashlib
import hmac
from typing import Optional, Tuple
from dataclasses import dataclass


# Dangerous modules that plugins MUST NOT import
BANNED_IMPORTS = {
    "os",
    "sys",
    "subprocess",
    "eval",
    "exec",
    "compile",
    "__import__",
    "importlib",
    "shutil",
    "socket",
    "requests",
    "urllib",
    "http",
    "ftplib",
    "smtplib",
    "pickle",
    "shelve",
    "marshal",
    "ctypes",
    "multiprocessing",
}

# Dangerous built-in functions
BANNED_BUILTINS = {
    "eval",
    "exec",
    "compile",
    "__import__",
    "open",  # File I/O should go through context
}


@dataclass
class ValidationResult:
    """Result of plugin safety validation."""

    is_safe: bool
    error_message: Optional[str] = None
    violations: list[str] = None

    def __post_init__(self):
        if self.violations is None:
            self.violations = []


def compute_source_hash(source_code: str) -> str:
    """
    Compute SHA-256 hash of source code (content-addressable).

    Args:
        source_code: Python source code

    Returns:
        64-character hex digest
    """
    return hashlib.sha256(source_code.encode("utf-8")).hexdigest()


def verify_signature(payload: str, signature: str, secret_key: str) -> bool:
    """
    Verify HMAC signature of payload.

    Args:
        payload: The data that was signed (usually source_code)
        signature: The HMAC hex digest to verify
        secret_key: Shared secret key

    Returns:
        True if signature is valid, False otherwise
    """
    expected_sig = hmac.new(
        secret_key.encode("utf-8"), payload.encode("utf-8"), hashlib.sha256
    ).hexdigest()

    # Constant-time comparison to prevent timing attacks
    return hmac.compare_digest(expected_sig, signature)


def validate_plugin_safety(source_code: str) -> ValidationResult:
    """
    Validate plugin source code using AST-based static analysis.

    Checks:
    1. Code is syntactically valid Python
    2. No dangerous imports (os, subprocess, etc.)
    3. No dangerous built-in calls (eval, exec, etc.)
    4. Must define a class that inherits from BasePlugin
    5. No __import__ tricks or dynamic imports

    Args:
        source_code: Python source code to validate

    Returns:
        ValidationResult with safety status and error details
    """
    violations = []

    # Step 1: Parse the code
    try:
        tree = ast.parse(source_code)
    except SyntaxError as e:
        return ValidationResult(
            is_safe=False, error_message=f"Syntax error: {e}", violations=["SYNTAX_ERROR"]
        )

    # Step 2: Check for dangerous imports
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name in BANNED_IMPORTS:
                    violations.append(f"Banned import: {alias.name}")

        elif isinstance(node, ast.ImportFrom):
            if node.module in BANNED_IMPORTS:
                violations.append(f"Banned import: from {node.module}")

        # Check for dangerous built-in calls
        elif isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name):
                if node.func.id in BANNED_BUILTINS:
                    violations.append(f"Banned built-in: {node.func.id}()")

    # Step 3: Ensure BasePlugin inheritance
    has_base_plugin = False
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            for base in node.bases:
                if isinstance(base, ast.Name) and base.id == "BasePlugin":
                    has_base_plugin = True
                    break

    if not has_base_plugin:
        violations.append("Plugin must define a class that inherits from BasePlugin")

    # Step 4: Compile results
    if violations:
        return ValidationResult(
            is_safe=False,
            error_message="; ".join(violations),
            violations=violations,
        )

    return ValidationResult(is_safe=True)


def generate_signature(source_code: str, secret_key: str) -> str:
    """
    Generate HMAC signature for source code.

    Args:
        source_code: Python source code
        secret_key: Shared secret key

    Returns:
        HMAC hex digest
    """
    return hmac.new(
        secret_key.encode("utf-8"), source_code.encode("utf-8"), hashlib.sha256
    ).hexdigest()
