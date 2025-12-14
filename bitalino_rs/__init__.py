"""
BITalino Rust driver with Python bindings.

This package provides a robust interface to BITalino biosignal acquisition
devices via Bluetooth RFCOMM, with automatic pairing and no root privileges required.

Quick Start:
    >>> import bitalino_rs
    >>> device = bitalino_rs.Bitalino.connect("98:D3:51:FE:6F:A3")
    >>> print(f"Firmware: {device.version()}")
    >>>
    >>> # Check battery (BITalino 2.0+ only)
    >>> if device.is_bitalino2:
    ...     state = device.state()
    ...     print(f"Battery: {state.battery_voltage:.2f}V")
    >>>
    >>> device.start(rate=1000, channels=[0, 1, 2])
    >>> frames = device.read(100)
    >>> device.stop()

Classes:
    Bitalino: Main device driver class.
    Frame: A single data frame with sequence, digital, and analog values.
    FrameBatch: Batch of frames with timing and error statistics.
    DeviceState: Device state information (BITalino 2.0+ only).

Constants:
    DEFAULT_SAMPLING_RATE: Default sampling rate (1000 Hz).
    VALID_SAMPLING_RATES: List of valid rates [1, 10, 100, 1000].
"""

from bitalino_rs._core import (
    Bitalino,
    Frame,
    FrameBatch,
    DeviceState,
    DEFAULT_SAMPLING_RATE,
    VALID_SAMPLING_RATES,
)

__all__ = [
    "Bitalino",
    "Frame",
    "FrameBatch",
    "DeviceState",
    "DEFAULT_SAMPLING_RATE",
    "VALID_SAMPLING_RATES",
]
__version__ = "0.1.0"
