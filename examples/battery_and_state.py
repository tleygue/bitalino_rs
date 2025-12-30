"""Check firmware, BITalino 2.0+ features, and current device state."""

from __future__ import annotations

import argparse

from bitalino_rs import Bitalino


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac", required=True, help="Bluetooth MAC address")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    dev = Bitalino.connect(args.mac)
    fw = dev.version()
    print(f"Firmware: {fw} | BITalino 2.0+: {dev.is_bitalino2}")

    if not dev.is_bitalino2:
        print("Device is not BITalino 2.0+; state/battery helpers unavailable")
        return 0

    state = dev.state()
    print(
        f"Battery: {state.battery_voltage:.2f}V | low: {state.is_battery_low} | "
        f"threshold: {state.battery_threshold}"
    )
    print(f"Analog (A1-A6): {state.analog}")
    print(f"Digital [I1, I2, O1, O2]: {state.digital}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
