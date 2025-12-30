"""Bridge Rust logs into Python logging and read a short batch."""

from __future__ import annotations

import argparse
import logging

from bitalino_rs import Bitalino, enable_rust_logs


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac", required=True, help="Bluetooth MAC address")
    parser.add_argument("--frames", type=int, default=200, help="Frames to read")
    parser.add_argument("--level", default="info", help="Rust/Python log level (e.g., debug, info)")
    return parser.parse_args()


def setup_logging(level: str) -> None:
    logging.basicConfig(level=level.upper(), format="%(levelname)s %(name)s: %(message)s")
    enable_rust_logs(level)


def main() -> int:
    args = parse_args()
    setup_logging(args.level)

    dev = Bitalino.connect(args.mac)
    dev.start(rate=1000, channels=[0, 1, 2])
    batch = dev.read_timed(args.frames)
    dev.stop()

    logging.info(
        "read %d frames; crc=%d gaps=%d", len(batch.frames), batch.crc_errors, batch.sequence_gaps
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
