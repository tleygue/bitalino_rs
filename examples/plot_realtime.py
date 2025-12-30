"""Live plot a channel while reading timed batches (requires matplotlib)."""

from __future__ import annotations

import argparse
import time

from bitalino_rs import Bitalino


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac", required=True, help="Bluetooth MAC address")
    parser.add_argument("--frames", type=int, default=200, help="Frames per batch")
    parser.add_argument("--duration", type=float, default=10.0, help="Duration to run (seconds)")
    parser.add_argument("--channel", type=int, default=0, help="Analog channel index to plot")
    return parser.parse_args()


def ensure_matplotlib():
    try:
        import matplotlib.pyplot as plt  # type: ignore
    except ImportError as exc:  # pragma: no cover - optional dep
        raise SystemExit("matplotlib is required: pip install matplotlib") from exc
    return plt


def main() -> int:
    args = parse_args()
    plt = ensure_matplotlib()

    dev = Bitalino.connect(args.mac)
    dev.start(rate=1000, channels=[0, 1, 2, 3])

    _fig, ax = plt.subplots()
    (line,) = ax.plot([], [], lw=1)
    ax.set_title(f"Analog channel {args.channel}")
    ax.set_xlabel("Sample")
    ax.set_ylabel("ADC (0-1023)")

    samples: list[int] = []
    start = time.time()

    while time.time() - start < args.duration:
        batch = dev.read_timed(args.frames)
        for frame in batch.frames:
            if args.channel < len(frame.analog):
                samples.append(frame.analog[args.channel])
        line.set_data(range(len(samples)), samples)
        ax.relim()
        ax.autoscale_view()
        plt.pause(0.01)

    dev.stop()
    plt.show()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
