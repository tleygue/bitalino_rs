"""Shared data structures for BITalino interactions.

These Python-visible types mirror what the Rust core returns: raw frames,
timed batches with integrity counters, and device state snapshots. The
``SamplingRate`` literal restricts configuration to values supported by the
hardware crystal (1, 10, 100, 1000 Hz).
"""

from typing import Literal

from bitalino_rs._bitalino_core import (
    DEFAULT_SAMPLING_RATE,
    VALID_SAMPLING_RATES,
    DeviceState,
    Frame,
    FrameBatch,
)

SamplingRate = Literal[1, 10, 100, 1000]

__all__ = [
    "DEFAULT_SAMPLING_RATE",
    "VALID_SAMPLING_RATES",
    "DeviceState",
    "Frame",
    "FrameBatch",
    "SamplingRate",
]
