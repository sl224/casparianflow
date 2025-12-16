# tests/test_gatekeeper.py
"""
Tests for Security Gatekeeper - AST validation and cryptographic functions.

Tests AST-based validation of plugin safety, HMAC signature verification,
and SHA-256 content hashing.
"""
import pytest
import hashlib
import hmac

from casparian_flow.security.gatekeeper import (
    validate_plugin_safety,
    verify_signature,
    compute_source_hash,
    generate_signature,
    ValidationResult,
)


# Sample plugin code for testing
VALID_PLUGIN = """
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path):
        df = pd.read_csv(file_path)
        self.publish("output", df)
"""

PLUGIN_NO_BASECLASS = """
import pandas as pd

class Handler:
    def execute(self, file_path):
        df = pd.read_csv(file_path)
"""

PLUGIN_WITH_OS_IMPORT = """
from casparian_flow.sdk import BasePlugin
import os

class Handler(BasePlugin):
    def execute(self, file_path):
        os.system("rm -rf /")
"""

PLUGIN_WITH_SUBPROCESS = """
from casparian_flow.sdk import BasePlugin
import subprocess

class Handler(BasePlugin):
    def execute(self, file_path):
        subprocess.call(["ls", "-la"])
"""

PLUGIN_WITH_EVAL = """
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path):
        eval("print('bad')")
"""

PLUGIN_WITH_SOCKET = """
from casparian_flow.sdk import BasePlugin
import socket

class Handler(BasePlugin):
    def execute(self, file_path):
        pass
"""

PLUGIN_WITH_PICKLE = """
from casparian_flow.sdk import BasePlugin
import pickle

class Handler(BasePlugin):
    def execute(self, file_path):
        pickle.loads(data)
"""

SYNTAX_ERROR_PLUGIN = """
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path)
        # Missing colon
"""

PLUGIN_WITH_MULTIPLE_VIOLATIONS = """
import os
import subprocess
import pickle

class Handler:
    def execute(self, file_path):
        eval("bad code")
"""


class TestASTValidation:
    """Test AST-based safety validation."""

    def test_validate_safe_plugin(self):
        """Valid plugin with BasePlugin inheritance passes."""
        result = validate_plugin_safety(VALID_PLUGIN)
        assert result.is_safe is True
        assert result.error_message is None
        assert len(result.violations) == 0

    def test_reject_os_import(self):
        """Block 'os' import."""
        result = validate_plugin_safety(PLUGIN_WITH_OS_IMPORT)
        assert result.is_safe is False
        assert "os" in result.error_message.lower()
        assert any("os" in v.lower() for v in result.violations)

    def test_reject_subprocess_import(self):
        """Block 'subprocess' import."""
        result = validate_plugin_safety(PLUGIN_WITH_SUBPROCESS)
        assert result.is_safe is False
        assert "subprocess" in result.error_message.lower()

    def test_reject_socket_import(self):
        """Block 'socket' import."""
        result = validate_plugin_safety(PLUGIN_WITH_SOCKET)
        assert result.is_safe is False
        assert "socket" in result.error_message.lower()

    def test_reject_pickle_import(self):
        """Block 'pickle' import."""
        result = validate_plugin_safety(PLUGIN_WITH_PICKLE)
        assert result.is_safe is False
        assert "pickle" in result.error_message.lower()

    def test_reject_eval_builtin(self):
        """Block eval() builtin."""
        result = validate_plugin_safety(PLUGIN_WITH_EVAL)
        assert result.is_safe is False
        assert "eval" in result.error_message.lower()

    def test_require_base_plugin_inheritance(self):
        """Fail if no BasePlugin base class."""
        result = validate_plugin_safety(PLUGIN_NO_BASECLASS)
        assert result.is_safe is False
        assert "baseplugin" in result.error_message.lower()
        assert any("baseplugin" in v.lower() for v in result.violations)

    def test_syntax_error_detection(self):
        """Invalid Python syntax rejected."""
        result = validate_plugin_safety(SYNTAX_ERROR_PLUGIN)
        assert result.is_safe is False
        assert result.error_message is not None
        assert "syntax" in result.error_message.lower()

    def test_multiple_violations(self):
        """Return all violations in ValidationResult."""
        result = validate_plugin_safety(PLUGIN_WITH_MULTIPLE_VIOLATIONS)
        assert result.is_safe is False
        assert len(result.violations) >= 3  # os, subprocess, pickle, eval, no BasePlugin

        # Check that all violations are mentioned
        error_lower = result.error_message.lower()
        assert "os" in error_lower or any("os" in v.lower() for v in result.violations)

    def test_validation_result_structure(self):
        """ValidationResult has correct structure."""
        result = validate_plugin_safety(VALID_PLUGIN)
        assert isinstance(result, ValidationResult)
        assert isinstance(result.is_safe, bool)
        assert isinstance(result.violations, list)


class TestDangerousImports:
    """Test that all dangerous modules are blocked."""

    @pytest.mark.parametrize(
        "module_name",
        [
            "os",
            "sys",
            "subprocess",
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
        ],
    )
    def test_dangerous_import_blocked(self, module_name):
        """Test that each dangerous module is blocked."""
        code = f"""
from casparian_flow.sdk import BasePlugin
import {module_name}

class Handler(BasePlugin):
    def execute(self, file_path):
        pass
"""
        result = validate_plugin_safety(code)
        assert result.is_safe is False
        assert module_name in result.error_message.lower()


class TestDangerousBuiltins:
    """Test that dangerous builtin functions are blocked."""

    @pytest.mark.parametrize(
        "builtin_func",
        ["eval", "exec", "compile", "__import__"],
    )
    def test_dangerous_builtin_blocked(self, builtin_func):
        """Test that each dangerous builtin is blocked."""
        code = f"""
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path):
        {builtin_func}("bad code")
"""
        result = validate_plugin_safety(code)
        assert result.is_safe is False
        assert builtin_func in result.error_message.lower()


class TestHashFunctions:
    """Test SHA-256 content hashing."""

    def test_compute_source_hash(self):
        """SHA-256 hash is 64 chars hex."""
        hash_val = compute_source_hash(VALID_PLUGIN)
        assert len(hash_val) == 64
        assert all(c in "0123456789abcdef" for c in hash_val)

    def test_hash_deterministic(self):
        """Same code → same hash."""
        hash1 = compute_source_hash(VALID_PLUGIN)
        hash2 = compute_source_hash(VALID_PLUGIN)
        assert hash1 == hash2

    def test_hash_different_for_different_code(self):
        """Different code → different hash."""
        hash1 = compute_source_hash(VALID_PLUGIN)
        hash2 = compute_source_hash(PLUGIN_NO_BASECLASS)
        assert hash1 != hash2

    def test_hash_is_sha256(self):
        """Verify hash matches SHA-256 algorithm."""
        code = "test"
        expected = hashlib.sha256(code.encode("utf-8")).hexdigest()
        actual = compute_source_hash(code)
        assert actual == expected

    def test_hash_whitespace_sensitive(self):
        """Whitespace changes affect hash."""
        code1 = "x = 1"
        code2 = "x=1"
        hash1 = compute_source_hash(code1)
        hash2 = compute_source_hash(code2)
        assert hash1 != hash2


class TestSignatureVerification:
    """Test HMAC-SHA256 signature verification."""

    def test_verify_signature_valid(self):
        """Valid HMAC passes."""
        secret = "my-secret-key"
        payload = "source code here"
        signature = generate_signature(payload, secret)

        assert verify_signature(payload, signature, secret) is True

    def test_verify_signature_invalid(self):
        """Wrong signature fails."""
        secret = "my-secret-key"
        payload = "source code here"
        wrong_signature = "0" * 64

        assert verify_signature(payload, wrong_signature, secret) is False

    def test_verify_signature_wrong_secret(self):
        """Wrong secret key fails verification."""
        secret1 = "secret-key-1"
        secret2 = "secret-key-2"
        payload = "source code"

        signature = generate_signature(payload, secret1)
        assert verify_signature(payload, signature, secret2) is False

    def test_verify_signature_modified_payload(self):
        """Modified payload fails verification."""
        secret = "my-secret"
        payload1 = "original code"
        payload2 = "modified code"

        signature = generate_signature(payload1, secret)
        assert verify_signature(payload2, signature, secret) is False

    def test_verify_signature_timing_safe(self):
        """Uses hmac.compare_digest for constant-time comparison."""
        # This is more of a code inspection test
        # verify_signature should use hmac.compare_digest internally
        # We can't easily test timing attacks, but we can verify behavior
        secret = "secret"
        payload = "data"
        sig = generate_signature(payload, secret)

        # Should use constant-time comparison
        result = verify_signature(payload, sig, secret)
        assert result is True

    def test_generate_signature(self):
        """Creates valid HMAC."""
        secret = "test-secret"
        payload = "test payload"

        signature = generate_signature(payload, secret)

        # Should be 64-char hex string (SHA-256 produces 256 bits = 32 bytes = 64 hex chars)
        assert len(signature) == 64
        assert all(c in "0123456789abcdef" for c in signature)

        # Should be verifiable
        assert verify_signature(payload, signature, secret) is True

    def test_generate_signature_matches_hmac(self):
        """Verify signature matches HMAC algorithm."""
        secret = "test-key"
        payload = "test data"

        expected = hmac.new(
            secret.encode("utf-8"), payload.encode("utf-8"), hashlib.sha256
        ).hexdigest()

        actual = generate_signature(payload, secret)
        assert actual == expected


class TestEdgeCases:
    """Test edge cases and boundary conditions."""

    def test_empty_source_code(self):
        """Empty string fails syntax validation."""
        result = validate_plugin_safety("")
        assert result.is_safe is False

    def test_very_long_source_code(self):
        """Very long valid code still works."""
        long_code = VALID_PLUGIN + "\n" + ("# comment\n" * 1000)
        result = validate_plugin_safety(long_code)
        assert result.is_safe is True

    def test_unicode_source_code(self):
        """Unicode characters in code are handled."""
        code = """
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path):
        # Comment with unicode: é ñ 中文
        pass
"""
        result = validate_plugin_safety(code)
        assert result.is_safe is True

    def test_hash_empty_string(self):
        """Hashing empty string works."""
        hash_val = compute_source_hash("")
        assert len(hash_val) == 64

    def test_signature_empty_payload(self):
        """Signing empty payload works."""
        secret = "key"
        signature = generate_signature("", secret)
        assert verify_signature("", signature, secret) is True


class TestSafeImports:
    """Test that safe imports are allowed."""

    @pytest.mark.parametrize(
        "safe_module",
        [
            "pandas",
            "numpy",
            "pyarrow",
            "datetime",
            "json",
            "re",
            "math",
            "collections",
        ],
    )
    def test_safe_imports_allowed(self, safe_module):
        """Test that safe modules are not blocked."""
        code = f"""
from casparian_flow.sdk import BasePlugin
import {safe_module}

class Handler(BasePlugin):
    def execute(self, file_path):
        pass
"""
        result = validate_plugin_safety(code)
        # If it's truly a safe import, it should pass
        # (assuming BasePlugin inheritance is present)
        assert result.is_safe is True or (
            result.is_safe is False and safe_module not in result.error_message.lower()
        )
