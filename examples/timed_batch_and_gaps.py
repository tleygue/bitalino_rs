"""Read a timed batch and inspect CRC/sequence gaps; reconstruct sample times."""

from __future__ import annotations

import argparse

from bitalino_rs import Bitalino, SamplingRate


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac", required=True, help="Bluetooth MAC address")
    parser.add_argument("--frames", type=int, default=500, help="Frames per batch")
    parser.add_argument(
        "--rate", type=int, default=1000, choices=[1, 10, 100, 1000], help="Sampling rate"
    )
    return parser.parse_args()


def reconstruct_timestamps(timestamp_us: int, rate: SamplingRate, n: int) -> list[float]:
    period = 1_000_000 / rate
    return [timestamp_us + i * period for i in range(n)]


def main() -> int:
    args = parse_args()
    dev = Bitalino.connect(args.mac)
    dev.start(rate=args.rate, channels=[0, 1, 2])

    batch = dev.read_timed(args.frames)
    dev.stop()

    times = reconstruct_timestamps(batch.timestamp_us, args.rate, len(batch.frames))
    print(
        f"Frames: {len(batch.frames)} | CRC errors: {batch.crc_errors} | "
        f"seq gaps: {batch.sequence_gaps}"
    )
    print(f"First 3 timestamps (us): {times[:3]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
