# Trust Guarantees

**Last Updated:** 2026-01-23

This document describes Casparian Flow's plugin trust model and security guarantees.

---

## Overview

Casparian Flow executes user-provided plugins (parsers) to transform data. The trust system ensures that only authorized code executes on your system.

## Trust Model

### Plugin Types

| Plugin Type | Default Trust | Signing Required | Isolation |
|-------------|---------------|------------------|-----------|
| **Python** | Allowed (with warning) | No | Process sandbox |
| **Native (Rust/C)** | Blocked | Yes | Process sandbox |

### Trust Modes

Currently supported mode:

- **`vault_signed_only`** (default): Requires signature verification for native plugins; Python plugins follow `allow_unsigned_python` setting.

---

## Configuration

Trust settings are configured in `~/.casparian_flow/config.toml`:

```toml
[trust]
# Trust mode (currently only "vault_signed_only" supported)
mode = "vault_signed_only"

# Allow unsigned Python plugins (default: true for development)
# Set to false in production to require signed Python plugins
allow_unsigned_python = false

# Allow unsigned native executables (default: false)
# WARNING: Setting this to true is a security risk
allow_unsigned_native = false

# List of allowed signer IDs (must have corresponding keys below)
allowed_signers = ["casparian_root_2026"]

# Trusted public keys (Ed25519, base64-encoded)
[trust.keys]
casparian_root_2026 = "BASE64_ENCODED_ED25519_PUBLIC_KEY"
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `mode` | string | `vault_signed_only` | Trust verification mode |
| `allow_unsigned_python` | bool | `false` | Allow unsigned Python plugins (logs warning) |
| `allow_unsigned_native` | bool | `false` | Allow unsigned native executables |
| `allowed_signers` | list | `[]` | Signer IDs authorized to sign plugins |
| `keys` | table | `{}` | Ed25519 public keys keyed by signer ID |

---

## Python Plugin Trust

### Development Mode (default)

When `allow_unsigned_python = true`:
- Unsigned Python plugins are **allowed to run**
- A warning is logged: `"Running unsigned Python plugin: {name}"`
- Suitable for local development and testing

### Production Mode

When `allow_unsigned_python = false`:
- Unsigned Python plugins are **blocked**
- Error message: `"Unsigned Python plugin blocked by trust policy"`
- Plugins must be signed by an authorized signer

### Path Traversal Protection

Python plugins are protected against path traversal attacks:
- Plugin entrypoints are validated via `validate_entrypoint()`
- Paths containing `..` or absolute paths outside the plugin directory are rejected
- This prevents plugins from accessing files outside their designated scope

---

## Native Plugin Trust

Native plugins (compiled executables) have stricter requirements:

### Signature Verification

- Native plugins **must** be signed unless `allow_unsigned_native = true`
- Signatures use Ed25519 with a detached `.sig` file
- The signer's public key must be in `trust.keys`
- The signer ID must be in `allowed_signers`

### Signature Format

```
plugin.exe          # The executable
plugin.exe.sig      # Detached Ed25519 signature
```

---

## Security Guarantees

### What We Guarantee

1. **Entrypoint Validation**: Plugins cannot access files outside their scope via path traversal
2. **Signature Verification**: Native plugins must be signed by trusted signers
3. **Process Isolation**: Plugins run in separate processes, not in the host process
4. **Configuration Validation**: Unknown config fields are rejected to prevent typos

### What We Don't Guarantee

1. **Sandbox Escape**: If a plugin has a vulnerability, it may access the host system
2. **Network Isolation**: Plugins can make network requests
3. **Resource Limits**: Plugins can consume unlimited CPU/memory (use OS limits)

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `CASPARIAN_HOME` | Override config directory (default: `~/.casparian_flow`) |

---

## Troubleshooting

### "Unsigned Python plugin blocked by trust policy"

**Cause**: `allow_unsigned_python = false` and plugin has no signature.

**Fix**: Either:
1. Sign the plugin with an authorized key
2. Set `allow_unsigned_python = true` (explicit dev-only override)

### "allowed_signer 'X' missing from trust.keys"

**Cause**: A signer ID is in `allowed_signers` but has no public key in `keys`.

**Fix**: Add the public key:
```toml
[trust.keys]
X = "BASE64_ENCODED_PUBLIC_KEY"
```

### "unknown field 'X'"

**Cause**: Typo in config field name. Unknown fields are rejected for security.

**Fix**: Check spelling against the documented options above.

---

## Implementation References

- Trust config: `crates/casparian/src/trust/config.rs`
- Path validation: `crates/casparian_worker/src/worker.rs::validate_entrypoint()`
- Signature verification: `crates/casparian_security/src/signing.rs`

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-23 | Initial trust documentation |
