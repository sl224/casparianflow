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
# Install dependencies
uv sync

# Publish a plugin
casparian publish ./my_plugin/

# Run the Sentinel (control plane)
uv run -m casparian_flow.main

# Run a Worker (data plane)
uv run -m casparian_flow.engine.worker_client --connect tcp://localhost:5555
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for the complete system design.

### Key Components

| Component | Description |
|-----------|-------------|
| **Publisher CLI** | End-to-end artifact publishing with signing |
| **Sentinel** | Control plane broker for job orchestration |
| **Worker** | Data plane executor with Bridge Mode support |
| **Architect** | Plugin deployment lifecycle management |
| **Scout** | File discovery and versioning service |
| **VenvManager** | Isolated environment lifecycle (LRU eviction) |

### Execution Modes

- **Legacy Mode**: Plugin runs in host process (shared deps, higher performance)
- **Bridge Mode**: Plugin runs in isolated venv subprocess (full isolation via Arrow IPC)

## Security

- **Local Mode**: Zero friction development with auto-generated Ed25519 keys
- **Enterprise Mode**: Azure AD integration with JWT validation
- **Gatekeeper**: AST-based validation blocks dangerous imports
- **Isolation**: Guest processes have no access to credentials

## Requirements

- Python 3.13+
- [uv](https://github.com/astral-sh/uv) package manager

## License

Proprietary
