"""Happy-path acquisition: connect, read, stop."""

from __future__ import annotations

import argparse

from bitalino_rs import Bitalino


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mac", required=True, help="Bluetooth MAC address (e.g., 12:34:56:78:9A:BC)"
    )
    parser.add_argument("--frames", type=int, default=100, help="Number of frames to read")
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    dev = Bitalino.connect(args.mac)
    dev.start(rate=1000, channels=[0, 1, 2])
    frames = dev.read(args.frames)
    dev.stop()

    print(f"Read {len(frames)} frames; first frame: {frames[0] if frames else 'n/a'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
