# bitalino-rs

Rust driver and Python bindings for BITalino biosignal acquisition devices. This project opens RFCOMM connections to pre-paired devices with a minimal libc-based stack, exposes a safe Rust API, and ships Python wheels for easy use in data pipelines.

## Quick Links
- Docs: [tleygue.github.io/bitalino_rs](https://tleygue.github.io/bitalino_rs) (published via GitHub Pages; sources in [docs/](docs/))
- Python package: `bitalino_rs` (built with maturin, published from tags)
- Crate: `bitalino-rs` (Rust library)

## Features
- Connect to BITalino over Bluetooth without root privileges (expects device is pre-paired/trusted).
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
Requirements: Rust (stable), Python 3.11+.
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
use std::time::Duration;
use bitalino_rs::{Bitalino, SamplingRate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
		let mut dev = Bitalino::connect("7E:91:2B:C4:AF:08", "1234")?;
		dev.start(SamplingRate::Hz1000 as u16, Some(vec![0, 1, 2]))?;
		dev.wait_until_streaming(Duration::from_secs(2))?; // block until BT link is reliable
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
dev.wait_until_streaming(timeout=2.0)  # block until BT link is reliable
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

### Logging
- Default level: `info`. Override with `BITALINO_LOG=debug` (falls back to `RUST_LOG` if unset).
- Rust binaries: call `bitalino_rs::init_rust_logging()` once (idempotent).
- Python: logging is wired on import; adjust from Python with `bitalino_rs.enable_rust_logs("debug")` or clear caches with `bitalino_rs.reset_log_cache()` after reconfiguring Python logging.

## License
Apache License 2.0. See [LICENSE](LICENSE).
