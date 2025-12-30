"""BITalino driver with Python bindings.

Layout and intent
-----------------
The Python package is a thin, well-typed façade over the Rust core compiled as
``_core``. Everything is grouped by concern:

* ``device``: High-level driver that wraps Bluetooth/serial I/O and exposes the
    stateful operations you call from Python.
* ``models``: Immutable data carriers (frames, batches, device state) and the
    allowed sampling-rate literals used across the API surface.
* ``logging``: Opt-in helpers that bridge Rust logs into Python's ``logging``
    ecosystem so you can watch the driver internals during debugging or capture
    them alongside your application logs.

What happens under the hood
---------------------------
The Rust layer (pyo3/abi3) owns the heavy lifting:

* Pairing/connect uses a Bluetooth RFCOMM stream (or serial path) and retries
    with helpful errors.
* Acquisition runs on the device crystal; the driver reconstructs timing from
    sequence numbers and host timestamps, surfacing CRC and gap counts via
    ``FrameBatch``.
* Battery/state helpers are available only on BITalino 2.0+ and guard against
    invalid modes.

Typical usage
-------------
>>> from bitalino_rs import Bitalino, enable_rust_logs
>>> enable_rust_logs("info")  # bridge Rust logs into Python's logging
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
