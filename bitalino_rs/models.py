"""Shared data structures for BITalino interactions.

Exports immutable data holders returned by the driver and sampling rate
constants that describe the valid acquisition frequencies.
"""

from typing import Literal

from bitalino_rs._core import (
    DEFAULT_SAMPLING_RATE,
    VALID_SAMPLING_RATES,
    DeviceState,
    Frame,
    FrameBatch,
)

SamplingRate = Literal[1, 10, 100, 1000]

__all__ = [
    "Frame",
    "FrameBatch",
    "DeviceState",
    "SamplingRate",
    "DEFAULT_SAMPLING_RATE",
    "VALID_SAMPLING_RATES",
]
