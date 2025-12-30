"""Retry connecting with exponential backoff and jitter."""

from __future__ import annotations

import argparse
import random
import time

from bitalino_rs import Bitalino


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac", required=True, help="Bluetooth MAC address")
    parser.add_argument("--retries", type=int, default=5, help="Maximum connection attempts")
    parser.add_argument("--base", type=float, default=0.5, help="Base backoff seconds")
    parser.add_argument("--cap", type=float, default=8.0, help="Maximum backoff seconds")
    parser.add_argument("--frames", type=int, default=100, help="Frames to read after connect")
    return parser.parse_args()


def backoff(base: float, cap: float, attempt: int) -> float:
    delay = min(cap, base * (2**attempt))
    jitter = random.uniform(0, delay * 0.25)
    return delay + jitter


def connect_with_retries(mac: str, retries: int, base: float, cap: float) -> Bitalino:
    last_error: Exception | None = None
    for attempt in range(retries):
        try:
            return Bitalino.connect(mac)
        except Exception as exc:  # pragma: no cover - runtime behavior
            last_error = exc
            wait = backoff(base, cap, attempt)
            print(f"connect failed ({exc}); retrying in {wait:.2f}s ({attempt+1}/{retries})")
            time.sleep(wait)
    raise SystemExit(f"failed to connect after {retries} attempts: {last_error}")


def main() -> int:
    args = parse_args()
    dev = connect_with_retries(args.mac, args.retries, args.base, args.cap)
    dev.start(rate=1000, channels=[0, 1, 2])
    frames = dev.read(args.frames)
    dev.stop()
    print(f"connected and read {len(frames)} frames")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
