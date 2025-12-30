"""Type stubs for the compiled BITalino core extension.

These stubs mirror the Rust implementation shipped as ``_core``. The extension
handles Bluetooth RFCOMM/serial transport, command encoding, CRC checking, and
timing reconstruction; Python sees ergonomic classes and functions with rich
type information for IDEs and static analyzers.
"""

from typing import Any, Literal

SamplingRate = Literal[1, 10, 100, 1000]

# Module constants
DEFAULT_SAMPLING_RATE: SamplingRate
VALID_SAMPLING_RATES: list[SamplingRate]

# Logging helpers
def enable_rust_logs(level: str | None = ...) -> None:
    """Enable Rust-side logging bridged into Python's ``logging`` module."""

def reset_log_cache() -> None:
    """Clear cached Python logger lookups (useful after logging reconfiguration)."""

class Frame:
    """Single BITalino frame (immutable, hashable).

    Under the hood, frames are emitted at the configured sampling rate by the
    device's crystal. Each frame carries a 4-bit sequence counter (0-15) so you
    can detect lost samples, plus the digital lines and the analog channels you
    asked for in ``start()``.

    Attributes:
        sequence: Frame sequence number (0-15, wraps). Discontinuities indicate drops.
        digital: Digital lines as [I1, I2, O1, O2] with values 0/1.
        analog: Analog channel values (10-bit, 0-1023) in channel order.
        n_channels: Number of analog channels present (len of ``analog``).
    """

    sequence: int
    digital: list[int]
    analog: list[int]
    n_channels: int

    def __new__(cls, sequence: int, digital: list[int], analog: list[int]) -> Frame: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for easy serialization."""
        ...

class FrameBatch:
    """Batch of frames with timing and integrity diagnostics.

    The Rust layer timestamps when the batch read begins and tallies integrity
    issues. CRC failures mean corrupted frames were dropped; sequence gaps mean
    frames were missing according to the 4-bit counter. Both are surfaced so you
    can decide whether to trust or re-acquire data.

    Attributes:
        frames: Frames delivered for this read call.
        timestamp_us: Microseconds since acquisition started at the moment of read.
        crc_errors: Count of frames dropped due to failed CRC.
        sequence_gaps: Count of discontinuities detected in the 4-bit sequence.
        has_errors: True if either CRC errors or sequence gaps occurred.
    """

    frames: list[Frame]
    timestamp_us: int
    crc_errors: int
    sequence_gaps: int
    has_errors: bool

    def __repr__(self) -> str: ...
    def __len__(self) -> int: ...

class DeviceState:
    """Snapshot of device status (BITalino 2.0+ only).

    Captured only when the device is idle. Useful for health checks without
    starting acquisition. Battery voltage is derived from the raw ADC using the
    official voltage-divider formula; the low-battery flag compares against the
    configured threshold (3.4-3.8V window).

    Attributes:
        analog: All 6 analog channel values (10-bit, 0-1023).
        battery: Battery ADC value (10-bit, 0-1023).
        battery_threshold: Current battery threshold setting (0-63).
        digital: Digital channel states [I1, I2, O1, O2].
        battery_voltage: Approximate battery voltage in Volts.
        is_battery_low: True if battery is below the configured threshold.
    """

    analog: list[int]
    battery: int
    battery_threshold: int
    digital: list[int]
    battery_voltage: float
    is_battery_low: bool

    def __repr__(self) -> str: ...
    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for easy serialization."""
        ...

class Bitalino:
    """Stateful BITalino driver exposed to Python.

    The Rust core holds the transport (Bluetooth RFCOMM or serial), encodes the
    wire protocol, enforces timing semantics, and normalizes errors into clear
    Python exceptions. Sampling is clocked by the device; the host reconstructs
    time using frame sequence numbers plus a host-side timestamp taken per read.

    Attributes:
        sampling_rate: Current sampling rate in Hz (one of 1, 10, 100, 1000).
        elapsed_us: Microseconds since acquisition started (None until started).
        is_bitalino2: True if device reports firmware >= 4.2 (enables extras).

    Example:
        >>> dev = Bitalino.connect("7E:91:2B:C4:AF:08")
        >>> fw = dev.version()
        >>> dev.start(rate=1000, channels=[0, 1, 2])
        >>> batch = dev.read_timed(500)
        >>> dev.stop()
    """

    sampling_rate: SamplingRate
    elapsed_us: int | None
    is_bitalino2: bool

    def __new__(cls, path: str) -> Bitalino:
        """
        Create a BITalino instance from a device path.

        Args:
            path: Path to the device (e.g., "/dev/rfcomm0" or "/dev/ttyUSB0")

        Returns:
            A new Bitalino instance

        Raises:
            IOError: If the device cannot be opened
        """
        ...

    @staticmethod
    def connect(address: str, pin: str = "1234") -> Bitalino:
        """
        Connect to a BITalino device via Bluetooth.

        Default (manual RFCOMM): expects the device already paired/trusted
        (e.g., via ``bluetoothctl``) and opens a raw RFCOMM socket using the MAC.
        Optional BlueZ mode (feature ``bluez``): discovers and pairs with the
        provided PIN, then opens an RFCOMM stream with retries.

        Args:
            address: Bluetooth MAC address (e.g., "7E:91:2B:C4:AF:08")
            pin: Pairing PIN code (default: "1234")

        Returns:
            A connected Bitalino instance

        Raises:
            ConnectionError: If the device cannot be found, paired, or connected
        """
        ...

    def version(self) -> str:
        """
        Get the BITalino firmware version string.

        Also sets ``is_bitalino2`` based on firmware (>= 4.2) to gate extended
        features like ``state()``, ``pwm()``, and idle ``trigger()``.

        Returns:
            Firmware version (e.g., "BITalino_v5.2")

        Raises:
            IOError: If communication fails
        """
        ...

    def start(self, rate: SamplingRate = 1000, channels: list[int] | None = None) -> None:
        """
        Start data acquisition.

        Args:
            rate: Sampling rate in Hz. Must be 1, 10, 100, or 1000. Default: 1000.
            channels: List of analog channels to acquire (0-5). Default: all channels.

        Under the hood, the driver configures the frame size for the selected
        channels and clears timing state so sequence deltas and timestamps are
        consistent for subsequent reads.

        Raises:
            ValueError: If rate or channels are invalid
            RuntimeError: If the device fails to start
        """
        ...

    def stop(self) -> None:
        """
        Stop data acquisition.

        Raises:
            RuntimeError: If the device fails to stop
        """
        ...

    def read(self, n_frames: int = 100) -> list[Frame]:
        """
        Read frames from the device.

        Args:
            n_frames: Number of frames to read. Default: 100.

        Returns:
            List of Frame objects containing the acquired data.
            May contain fewer frames than requested if CRC errors occur.

        Under the hood, CRC-failed frames are dropped; sequence gaps are not
        surfaced here—use ``read_timed`` for diagnostics.

        Raises:
            IOError: If reading fails
        """
        ...

    def read_timed(self, n_frames: int = 100) -> FrameBatch:
        """
        Read frames with timing and error information.

        This is the recommended method for high-quality acquisition as it provides
        timing information for sample reconstruction and error statistics.

        Args:
            n_frames: Number of frames to read. Default: 100.

        Returns:
            FrameBatch with frames, timestamp_us, crc_errors, and sequence_gaps.

        Under the hood, the batch timestamp is captured before reading to avoid
        Bluetooth buffering skew; CRC failures and sequence discontinuities are
        tallied for you.

        Raises:
            IOError: If reading fails
        """
        ...

    def set_battery_threshold(self, threshold: int = 30) -> None:
        """
        Set the battery threshold level.

        When battery voltage drops below this threshold, the device LED will blink.
        Must be called when not in acquisition mode.

        Args:
            threshold: Threshold value (0-63).
                0 = 3.4V (minimum), 63 = 3.8V (maximum).
                Default: 30 (~3.6V)

        Raises:
            RuntimeError: If device is currently in acquisition mode
        """
        ...

    def battery(self, value: int = 30) -> None:
        """
        Alias for set_battery_threshold() for compatibility with official library.

        Args:
            value: Threshold value (0-63). Default: 30.

        Raises:
            RuntimeError: If device is currently in acquisition mode
        """
        ...

    def state(self) -> DeviceState:
        """
        Get the current device state (BITalino 2.0+ only).

        Returns the current values of all analog channels, digital channels,
        battery level, and battery threshold. Must be called when not in acquisition.

        Returns:
            DeviceState object with analog, battery, battery_threshold, digital,
            battery_voltage, and is_battery_low properties.

        Raises:
            RuntimeError: If device is not BITalino 2.0+ or in acquisition mode
            IOError: If communication fails or CRC error
        """
        ...

    def trigger(self, outputs: list[int] | None = None) -> None:
        """
        Set digital output pins.

        Controls the digital output pins for external circuits or LED control.

        Args:
            outputs: List of output values (0 or 1).
                BITalino 2.0: [O1, O2] - works in both idle and acquisition modes
                BITalino 1.0: [O1, O2, O3, O4] - requires acquisition mode
                Default: [0, 0]

        Raises:
            RuntimeError: If BITalino 1.0 and not in acquisition mode
            ValueError: If outputs length/value is invalid for the device type
        """
        ...

    def pwm(self, value: int = 100) -> None:
        """
        Set PWM output value (BITalino 2.0+ only).

        Controls the pulse-width modulation output for LED brightness
        or other PWM-driven devices.

        Args:
            value: PWM duty cycle (0-255).
                0 = 0% duty cycle (always off)
                255 = 100% duty cycle (always on)
                Default: 100 (~39% duty cycle)

        Raises:
            RuntimeError: If device is not BITalino 2.0+
            ValueError: If value is outside 0-255
        """
        ...

    def __repr__(self) -> str: ...
