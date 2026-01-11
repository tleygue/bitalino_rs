# Installation

## Prerequisites

- Python 3.11 or newer
- Rust toolchain with `cargo`
- Bluetooth adapter on Linux

Transport requirements:
- **Default (manual RFCOMM)**: device must already be paired/trusted (e.g., via `bluetoothctl`) and you must know the MAC. No BlueZ daemon required at runtime.
- **BlueZ mode**: enable Cargo feature `bluez` to auto-discover/pair/connect; requires BlueZ daemon and dbus headers at build time.

## Install from PyPI

```bash
uv add bitalino-rs
```

By default this installs the manual RFCOMM backend. To build the Python wheel with BlueZ support instead:

```bash
UV_NO_SYNC=1 cargo build --features bluez
```

## Develop from source (uv-first)

```bash
git clone https://github.com/tleygue/bitalino_rs.git
cd bitalino_rs
uv sync          # installs Python tooling declared in pyproject
```

Build with manual RFCOMM (default):

```bash
cargo build --release
```

Build with BlueZ auto-pairing:

```bash
cargo build --release --features bluez
```
