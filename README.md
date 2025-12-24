# Casparian Flow

An Enterprise Artifact Registry with Bridge Mode Execution for data processing pipelines.

## Overview

Casparian Flow transforms "dark data" (files on disk) into structured, queryable datasets through:

- **Bridge Mode Execution**: Host/Guest privilege separation via isolated virtual environments
- **Publish-to-Execute Lifecycle**: Signed artifacts with Ed25519, auto-wired routing
- **Immutable Versioning**: Every file change creates a traceable version
- **Code-First Configuration**: Plugin source code defines routing and schemas

## Quick Start

```bash
# Build the unified binary
cargo build --release

# Run both Sentinel and Worker (unified process)
./target/release/casparian start

# Publish a plugin (creates signed artifact)
./target/release/casparian publish my_plugin.py --version 1.0.0
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for the complete system design.

### Key Components

| Component | Description |
|-----------|-------------|
| **Publisher CLI** | End-to-end artifact publishing with Ed25519 signing |
| **Sentinel** | Control plane broker for job orchestration |
| **Worker** | Data plane executor (Bridge Mode only) |
| **Architect** | Plugin deployment lifecycle management |
| **Scout** | File discovery and versioning service |
| **VenvManager** | Isolated environment lifecycle (LRU eviction) |

### Execution Model

- **Bridge Mode Only**: All plugins run in isolated venv subprocesses
- **Auto-Lockfile**: `uv.lock` auto-generated if missing
- **Arrow IPC**: Data streams via AF_UNIX sockets
- **Lineage Tracking**: `file_version_id` flows to guest process
- **Zero Trust**: Guest has no credentials, no heavy drivers

## Security

- **Local Mode**: Zero friction development with auto-generated Ed25519 keys
- **Enterprise Mode**: Azure AD integration with JWT validation
- **Gatekeeper**: AST-based validation blocks dangerous imports
- **Isolation**: Guest processes have no access to credentials

## Requirements

- Rust 1.75+ (for building from source)
- [uv](https://github.com/astral-sh/uv) (for plugin venv management)

## License

Proprietary
