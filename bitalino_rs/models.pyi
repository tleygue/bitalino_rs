"""Typed data carriers returned by the BITalino driver.

Mirrors the structures produced by the Rust core: raw frames, batches with
timing/error metadata, device state snapshots, and the sampling rate literals
accepted by the driver.
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
