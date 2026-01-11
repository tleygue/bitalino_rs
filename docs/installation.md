# Installation

## Prerequisites

- Python 3.11 or newer
- Rust toolchain with `cargo`
- Bluetooth adapter on Linux

Transport requirements:
- Device must already be paired/trusted (e.g., via `bluetoothctl`) and you must know the MAC. No BlueZ daemon or tokio/dbus stack is required.

## Install from PyPI

```bash
uv add bitalino-rs
```

This installs the minimal RFCOMM backend used for both the crate and the Python wheel.

## Develop from source (uv-first)

```bash
git clone https://github.com/tleygue/bitalino_rs.git
cd bitalino_rs
uv sync          # installs Python tooling declared in pyproject
```

Build the crate/wheel (minimal RFCOMM backend):

```bash
cargo build --release
```
