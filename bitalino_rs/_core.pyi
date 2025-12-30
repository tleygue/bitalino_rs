"""Type stubs for the compiled BITalino core extension."""

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
    """
    A single BITalino data frame (immutable, hashable).

    Attributes:
        sequence: Frame sequence number (0-15, wrapping). Use to detect dropped frames.
        digital: Digital channel values [I1, I2, O1, O2] as list of 0/1.
        analog: Analog channel values (10-bit, 0-1023).
        n_channels: Number of analog channels in this frame.
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
    """
    Result from reading a batch of frames, includes timing and error info.

    Attributes:
        frames: List of Frame objects.
        timestamp_us: Microseconds since acquisition started when batch was read.
        crc_errors: Number of frames discarded due to CRC errors.
        sequence_gaps: Number of detected dropped frames (from sequence discontinuities).
        has_errors: True if any CRC errors or sequence gaps occurred.
    """

    frames: list[Frame]
    timestamp_us: int
    crc_errors: int
    sequence_gaps: int
    has_errors: bool

    def __repr__(self) -> str: ...
    def __len__(self) -> int: ...

class DeviceState:
    """
    Device state information (BITalino 2.0+ only).

    Contains current values of all analog/digital channels and battery status.
    Obtained by calling Bitalino.state() when not in acquisition mode.

    Attributes:
        analog: All 6 analog channel values (10-bit, 0-1023).
        battery: Battery ADC value (10-bit, 0-1023).
        battery_threshold: Current battery threshold setting (0-63).
        digital: Digital channel states [I1, I2, O1, O2].
        battery_voltage: Approximate battery voltage in Volts.
        is_battery_low: True if battery is below threshold setting.
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
    """
    BITalino device driver.

    Provides an interface to BITalino biosignal acquisition devices via
    Bluetooth RFCOMM or serial connections. Supports automatic pairing
    without root privileges.

    Attributes:
        sampling_rate: Currently configured sampling rate in Hz.
        elapsed_us: Microseconds since acquisition started (None if not started).
        is_bitalino2: True if device is BITalino 2.0+ (supports extended features).

    Example:
        >>> device = Bitalino.connect("7E:91:2B:C4:AF:08")
        >>> print(f"Firmware: {device.version()}")
        >>> print(f"BITalino 2.0: {device.is_bitalino2}")
        >>>
        >>> # Check battery state (BITalino 2.0+ only)
        >>> if device.is_bitalino2:
        ...     state = device.state()
        ...     print(f"Battery: {state.battery_voltage:.2f}V")
        >>>
        >>> device.start(rate=1000, channels=[0, 1, 2])
        >>> batch = device.read_timed(1000)
        >>> print(f"Got {len(batch)} frames, {batch.crc_errors} errors")
        >>> device.stop()
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

        Automatically discovers the device, pairs with the given PIN,
        and establishes an RFCOMM connection. No root privileges required.
        Includes automatic retry logic for flaky connections.

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

        Also detects if device is BITalino 2.0+ (sets is_bitalino2 property).

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
        """
        ...

    def __repr__(self) -> str: ...
