"""BITalino driver with Python bindings.

The public surface is intentionally small and organized by concern:

``device``
    High-level driver to connect, configure, and read from a BITalino device.

``models``
    Immutable data carriers for frames, batches, and device state, plus sampling
    rate constants used across the API.

``logging``
    Opt-in helpers to enable or reset Rust-side logging bridged into Python's
    ``logging`` module.

Typical usage:
    >>> from bitalino_rs import Bitalino, enable_rust_logs
    >>> enable_rust_logs("info")
    >>> dev = Bitalino.connect("12:D3:51:FE:6F:A3")
    >>> dev.start(rate=1000, channels=[0, 1, 2])
    >>> frames = dev.read(100)
    >>> dev.stop()
"""

from bitalino_rs.device import Bitalino
from bitalino_rs.logging import enable_rust_logs, reset_log_cache
from bitalino_rs.models import (
    DEFAULT_SAMPLING_RATE,
    VALID_SAMPLING_RATES,
    DeviceState,
    Frame,
    FrameBatch,
    SamplingRate,
)

__all__ = [
    "Bitalino",
    "Frame",
    "FrameBatch",
    "DeviceState",
    "SamplingRate",
    "DEFAULT_SAMPLING_RATE",
    "VALID_SAMPLING_RATES",
    "enable_rust_logs",
    "reset_log_cache",
]
