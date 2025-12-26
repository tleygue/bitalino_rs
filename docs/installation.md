# Installation

## Prerequisites

- Python 3.11 or newer
- Rust toolchain with `cargo`
- Bluetooth adapter with BlueZ on Linux (required for RFCOMM access)

## Install from PyPI

```bash
uv add bitalino-rs
```

## Develop from source (uv-first)

```bash
git clone https://github.com/tleygue/bitalino_rs.git
cd bitalino_rs
uv sync          # installs Python tooling declared in pyproject
```
