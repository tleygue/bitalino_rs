# bitalino-rs

Rust driver and Python bindings for BITalino biosignal acquisition devices. This project automates Bluetooth pairing/connection (RFCOMM), exposes a safe Rust API, and ships Python wheels for easy use in data pipelines.

## Quick Links
- Docs: [docs/](docs/) (Rust + Python API notes)
- Python package: `bitalino_rs` (built with maturin, published from tags)
- Crate: `bitalino-rs` (Rust library)

## Features
- Connect to BITalino over Bluetooth without root privileges (automatic pair + bind).
- High-level Rust API plus generated Python bindings via PyO3/maturin.
- Timing-aware reads with sequence numbers, CRC tracking, and batch timestamps.
- Minimal dependencies; Ubuntu support verified in CI.

## Project Layout
- `src/` – Rust library (Bluetooth, driver, Python bindings)
- `bitalino_rs/` – Python package stub for maturin builds
- `docs/` – User/developer docs (mkdocs)
- `.github/workflows/` – CI, release, PyPI publish

## Install
### Python (from PyPI, when released)
```bash
uv pip install bitalino-rs
```

### Python (from source)
Requirements: Rust (stable), Python 3.11+, system deps (`libdbus-1-dev libudev-dev` on Ubuntu).
```bash
# in repo root
uv sync
```

### Rust crate (from source)
```bash
cargo build --release
```

## Usage
### Rust
```rust
use bitalino_rs::{Bitalino, SamplingRate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
		let mut dev = Bitalino::connect("7E:91:2B:C4:AF:08", "1234")?;
		dev.start(SamplingRate::Hz1000 as u16, Some(vec![0, 1, 2]))?;
		let frames = dev.read_frames(100)?;
		println!("read {} frames", frames.len());
		dev.stop()?;
		Ok(())
}
```

### Python
```python
from bitalino_rs import Bitalino

dev = Bitalino.connect("7E:91:2B:C4:AF:08")
dev.start(rate=1000, channels=[0, 1, 2])
batch = dev.read_timed(200)
print(batch.timestamp_us, batch.sequence_gaps)
dev.stop()
```

## Development
- Rust toolchain: `rustup toolchain install stable` (CI uses stable with rustfmt/clippy)
- System deps (Linux): `sudo apt-get install -y pkg-config libdbus-1-dev libudev-dev`
- Lint/format: `pre-commit run --all-files`
- Commit style: Conventional Commits (checked in CI)
- Tests: `cargo test --all-features --all-targets`

## Release & Packaging
- Tagging `v*` triggers GitHub Actions to build:
	- Rust release binary (Linux x86_64) attached to the GitHub Release
	- Python wheels/sdist built with `uv`/maturin, attached to the Release and published to PyPI via OIDC

## License
MIT License. See [LICENSE](LICENSE).
