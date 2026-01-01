"""Stream timed batches to CSV with timestamps and integrity columns."""

from __future__ import annotations

import argparse
import csv
import time
from pathlib import Path

from bitalino_rs import Bitalino


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac", required=True, help="Bluetooth MAC address")
    parser.add_argument("--frames", type=int, default=200, help="Frames per batch")
    parser.add_argument("--batches", type=int, default=10, help="Number of batches to write")
    parser.add_argument(
        "--out", type=Path, default=Path("bitalino_stream.csv"), help="CSV output path"
    )
    parser.add_argument(
        "--rate", type=int, default=1000, choices=[1, 10, 100, 1000], help="Sampling rate"
    )
    return parser.parse_args()


def write_header(writer: csv.writer) -> None:
    writer.writerow(
        [
            "batch_timestamp_us",
            "frame_index",
            "sequence",
            "digital",
            "analog",
            "crc_errors_in_batch",
            "sequence_gaps_in_batch",
        ]
    )


def main() -> int:
    args = parse_args()
    dev = Bitalino.connect(args.mac)
    dev.start(rate=args.rate, channels=[0, 1, 2, 3])

    with args.out.open("w", newline="") as f:
        writer = csv.writer(f)
        write_header(writer)

        for batch_idx in range(args.batches):
            batch = dev.read_timed(args.frames)
            for i, frame in enumerate(batch.frames):
                writer.writerow(
                    [
                        batch.timestamp_us,
                        i,
                        frame.sequence,
                        frame.digital,
                        frame.analog,
                        batch.crc_errors,
                        batch.sequence_gaps,
                    ]
                )
            print(
                f"batch {batch_idx + 1}/{args.batches}: frames={len(batch.frames)} "
                f"crc={batch.crc_errors} gaps={batch.sequence_gaps}"
            )
            time.sleep(0.05)

    dev.stop()
    print(f"Wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
