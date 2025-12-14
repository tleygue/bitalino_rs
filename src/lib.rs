//! BITalino Rust driver with Python bindings.
//!
//! This crate provides a robust interface to BITalino biosignal acquisition devices
//! via Bluetooth RFCOMM, with automatic pairing and no root privileges required.
//!
//! # Timing and Synchronization
//!
//! The BITalino device samples at a precise rate controlled by its internal crystal.
//! However, Bluetooth introduces variable latency. For accurate timing reconstruction:
//!
//! 1. Record the start time when calling `start()`
//! 2. Use sequence numbers to detect dropped frames
//! 3. Calculate sample times as: `start_time + sample_index / sampling_rate`

use pyo3::prelude::*;
use pyo3::types::PyDict;

mod bitalino;
mod bluetooth;
mod errors;

pub use bitalino::{Bitalino, DeviceState, Frame, FrameBatch, SamplingRate};
pub use bluetooth::{BluetoothConnector, RfcommStream};
pub use errors::*;

// ============================================================================
// Python Bindings
// ============================================================================

/// A single BITalino data frame (dataclass-like).
///
/// Attributes:
///     sequence: Frame sequence number (0-15, wrapping). Use to detect dropped frames.
///     digital: Digital input values [I1, I2, O1, O2] as list of 0/1.
///     analog: Analog channel values (10-bit, 0-1023) for configured channels.
#[pyclass(name = "Frame", frozen, eq)]
#[derive(Clone, PartialEq, Eq)]
struct PyFrame {
    #[pyo3(get)]
    sequence: u8,
    #[pyo3(get)]
    digital: Vec<u8>,
    #[pyo3(get)]
    analog: Vec<u16>,
}

#[pymethods]
impl PyFrame {
    #[new]
    fn new(sequence: u8, digital: Vec<u8>, analog: Vec<u16>) -> Self {
        PyFrame {
            sequence,
            digital,
            analog,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Frame(sequence={}, digital={:?}, analog={:?})",
            self.sequence, self.digital, self.analog
        )
    }

    fn __str__(&self) -> String {
        format!(
            "Frame(seq={}, d={:?}, a={:?})",
            self.sequence, self.digital, self.analog
        )
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.sequence.hash(&mut hasher);
        self.digital.hash(&mut hasher);
        self.analog.hash(&mut hasher);
        hasher.finish()
    }

    /// Convert to dictionary for easy serialization.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("sequence", self.sequence)?;
        dict.set_item("digital", self.digital.clone())?;
        dict.set_item("analog", self.analog.clone())?;
        Ok(dict)
    }

    /// Number of analog channels in this frame.
    #[getter]
    fn n_channels(&self) -> usize {
        self.analog.len()
    }
}

impl From<Frame> for PyFrame {
    fn from(f: Frame) -> Self {
        PyFrame {
            sequence: f.seq,
            digital: f.digital.to_vec(),
            analog: f.analog,
        }
    }
}

/// Result from reading a batch of frames, includes timing info.
///
/// Attributes:
///     frames: List of Frame objects.
///     timestamp_us: Microseconds since acquisition started when batch was read.
///     crc_errors: Number of frames discarded due to CRC errors.
///     sequence_gaps: Number of detected dropped frames (from sequence discontinuities).
#[pyclass(name = "FrameBatch", frozen)]
#[derive(Clone)]
struct PyFrameBatch {
    #[pyo3(get)]
    frames: Vec<PyFrame>,
    #[pyo3(get)]
    timestamp_us: u64,
    #[pyo3(get)]
    crc_errors: usize,
    #[pyo3(get)]
    sequence_gaps: usize,
}

#[pymethods]
impl PyFrameBatch {
    fn __repr__(&self) -> String {
        format!(
            "FrameBatch(frames={}, timestamp_us={}, crc_errors={}, sequence_gaps={})",
            self.frames.len(),
            self.timestamp_us,
            self.crc_errors,
            self.sequence_gaps
        )
    }

    fn __len__(&self) -> usize {
        self.frames.len()
    }

    /// Check if any errors occurred during reading.
    #[getter]
    fn has_errors(&self) -> bool {
        self.crc_errors > 0 || self.sequence_gaps > 0
    }
}

impl From<FrameBatch> for PyFrameBatch {
    fn from(b: FrameBatch) -> Self {
        PyFrameBatch {
            frames: b.frames.into_iter().map(PyFrame::from).collect(),
            timestamp_us: b.timestamp_us,
            crc_errors: b.crc_errors,
            sequence_gaps: b.sequence_gaps,
        }
    }
}

/// Device state information (BITalino 2.0+ only).
///
/// Contains current values of all analog/digital channels and battery status.
/// Obtained by calling Bitalino.state() when not in acquisition mode.
///
/// Attributes:
///     analog: All 6 analog channel values (10-bit, 0-1023).
///     battery: Battery ADC value (10-bit, 0-1023).
///     battery_threshold: Current battery threshold setting (0-63).
///     digital: Digital channel states [I1, I2, O1, O2].
#[pyclass(name = "DeviceState", frozen)]
#[derive(Clone)]
struct PyDeviceState {
    #[pyo3(get)]
    analog: Vec<u16>,
    #[pyo3(get)]
    battery: u16,
    #[pyo3(get)]
    battery_threshold: u8,
    #[pyo3(get)]
    digital: Vec<u8>,
}

#[pymethods]
impl PyDeviceState {
    fn __repr__(&self) -> String {
        format!(
            "DeviceState(battery={}, threshold={}, analog={:?}, digital={:?})",
            self.battery, self.battery_threshold, self.analog, self.digital
        )
    }

    /// Get the approximate battery voltage.
    ///
    /// Returns:
    ///     Approximate battery voltage in Volts (typically 3.2V - 4.2V).
    #[getter]
    fn battery_voltage(&self) -> f32 {
        (self.battery as f32 / 1023.0) * 3.3 * 2.0
    }

    /// Check if battery is low based on threshold setting.
    ///
    /// Returns:
    ///     True if battery voltage is below the threshold.
    #[getter]
    fn is_battery_low(&self) -> bool {
        let threshold_voltage = 3.4 + (self.battery_threshold as f32 / 63.0) * 0.4;
        self.battery_voltage() < threshold_voltage
    }

    /// Convert to dictionary for easy serialization.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("analog", self.analog.clone())?;
        dict.set_item("battery", self.battery)?;
        dict.set_item("battery_threshold", self.battery_threshold)?;
        dict.set_item("digital", self.digital.clone())?;
        dict.set_item("battery_voltage", self.battery_voltage())?;
        dict.set_item("is_battery_low", self.is_battery_low())?;
        Ok(dict)
    }
}

impl From<DeviceState> for PyDeviceState {
    fn from(s: DeviceState) -> Self {
        PyDeviceState {
            analog: s.analog.to_vec(),
            battery: s.battery,
            battery_threshold: s.battery_threshold,
            digital: s.digital.to_vec(),
        }
    }
}

/// BITalino device driver.
///
/// Provides methods to connect, configure, and read biosignal data from
/// BITalino devices via Bluetooth. No root privileges required.
///
/// Example:
///     >>> device = Bitalino.connect("98:D3:51:FE:6F:A3")
///     >>> print(f"Firmware: {device.version()}")
///     >>> device.start(rate=1000, channels=[0, 1, 2])
///     >>> frames = device.read(100)
///     >>> device.stop()
#[pyclass(name = "Bitalino", unsendable)]
struct PyBitalino {
    inner: Bitalino,
    sampling_rate: u16,
}

#[pymethods]
impl PyBitalino {
    /// Connect to a BITalino device via serial port path (e.g., `/dev/rfcomm0`).
    ///
    /// Use this if you've already paired and bound the device manually.
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        Bitalino::connect_serial(path)
            .map(|dev| PyBitalino {
                inner: dev,
                sampling_rate: 1000,
            })
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()))
    }

    /// Connect to a BITalino device via Bluetooth.
    ///
    /// Automatically discovers, pairs (if needed), and connects to the device.
    /// No root privileges required.
    ///
    /// Args:
    ///     mac: The MAC address of the device (e.g., "98:D3:51:FE:6F:A3")
    ///     pin: The PIN code for pairing (default: "1234")
    ///
    /// Returns:
    ///     A connected Bitalino instance
    ///
    /// Raises:
    ///     ConnectionError: If pairing or connection fails after retries
    #[staticmethod]
    #[pyo3(signature = (mac, pin="1234"))]
    fn connect(mac: &str, pin: &str) -> PyResult<Self> {
        let connector = BluetoothConnector::default();
        let stream = connector
            .pair_and_connect(mac, pin)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyConnectionError, _>(e.to_string()))?;

        Ok(PyBitalino {
            inner: Bitalino::from_rfcomm(stream),
            sampling_rate: 1000,
        })
    }

    /// Get the device firmware version.
    ///
    /// Returns:
    ///     Firmware version string (e.g., "BITalino_v5.2")
    fn version(&mut self) -> PyResult<String> {
        self.inner
            .version()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()))
    }

    /// Start data acquisition.
    ///
    /// Args:
    ///     rate: Sampling rate in Hz. Must be 1, 10, 100, or 1000. Default: 1000.
    ///     channels: List of analog channels to acquire (0-5). Default: all channels.
    ///
    /// Raises:
    ///     RuntimeError: If starting acquisition fails
    #[pyo3(signature = (rate=1000, channels=None))]
    fn start(&mut self, rate: u16, channels: Option<Vec<u8>>) -> PyResult<()> {
        let channels = channels.unwrap_or_else(|| vec![0, 1, 2, 3, 4, 5]);
        self.sampling_rate = rate;
        self.inner
            .start(rate, channels)
            .map(|_| ())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Stop data acquisition.
    fn stop(&mut self) -> PyResult<()> {
        self.inner
            .stop()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Read frames from the device.
    ///
    /// Args:
    ///     n_frames: Number of frames to read. Default: 100.
    ///
    /// Returns:
    ///     List of Frame objects with sequence, digital, and analog attributes.
    ///
    /// Raises:
    ///     IOError: If reading fails
    #[pyo3(signature = (n_frames=100))]
    fn read(&mut self, n_frames: usize) -> PyResult<Vec<PyFrame>> {
        self.inner
            .read_frames(n_frames)
            .map(|frames| frames.into_iter().map(PyFrame::from).collect())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()))
    }

    /// Read frames with timing and error information.
    ///
    /// This is the recommended method for high-quality acquisition as it provides
    /// timing information for sample reconstruction and error statistics.
    ///
    /// Args:
    ///     n_frames: Number of frames to read.
    ///
    /// Returns:
    ///     FrameBatch with frames, timestamp_us, crc_errors, and sequence_gaps.
    #[pyo3(signature = (n_frames=100))]
    fn read_timed(&mut self, n_frames: usize) -> PyResult<PyFrameBatch> {
        self.inner
            .read_frames_timed(n_frames)
            .map(PyFrameBatch::from)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()))
    }

    /// Get the current sampling rate.
    #[getter]
    fn sampling_rate(&self) -> u16 {
        self.sampling_rate
    }

    /// Get microseconds elapsed since acquisition started.
    #[getter]
    fn elapsed_us(&self) -> Option<u64> {
        self.inner.elapsed_us()
    }

    /// Check if this is a BITalino 2.0+ device.
    ///
    /// BITalino 2.0+ supports additional features like state(), pwm(), and
    /// trigger() in idle mode. Call version() first to detect device type.
    ///
    /// Returns:
    ///     True if device is BITalino 2.0+
    #[getter]
    fn is_bitalino2(&self) -> bool {
        self.inner.is_bitalino2()
    }

    /// Set the battery threshold level.
    ///
    /// When battery voltage drops below this threshold, the device LED will blink.
    /// Must be called when not in acquisition mode.
    ///
    /// Args:
    ///     threshold: Threshold value (0-63).
    ///         0 = 3.4V (minimum), 63 = 3.8V (maximum)
    ///
    /// Raises:
    ///     RuntimeError: If device is currently in acquisition mode
    #[pyo3(signature = (threshold=30))]
    fn set_battery_threshold(&mut self, threshold: u8) -> PyResult<()> {
        self.inner
            .set_battery_threshold(threshold)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Alias for set_battery_threshold for compatibility with official library.
    #[pyo3(signature = (value=30))]
    fn battery(&mut self, value: u8) -> PyResult<()> {
        self.set_battery_threshold(value)
    }

    /// Get the current device state (BITalino 2.0+ only).
    ///
    /// Returns the current values of all analog channels, digital channels,
    /// battery level, and battery threshold. Must be called when not in acquisition.
    ///
    /// Returns:
    ///     DeviceState object with analog, battery, battery_threshold, digital,
    ///     battery_voltage, and is_battery_low properties.
    ///
    /// Raises:
    ///     RuntimeError: If device is not BITalino 2.0+ or in acquisition mode
    ///     IOError: If communication fails or CRC error
    fn state(&mut self) -> PyResult<PyDeviceState> {
        self.inner
            .state()
            .map(PyDeviceState::from)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Set digital output pins.
    ///
    /// Controls the digital output pins for external circuits or LED control.
    ///
    /// Args:
    ///     outputs: List of output values (0 or 1).
    ///         BITalino 2.0: [O1, O2] - works in both idle and acquisition modes
    ///         BITalino 1.0: [O1, O2, O3, O4] - requires acquisition mode
    ///
    /// Raises:
    ///     RuntimeError: If BITalino 1.0 and not in acquisition mode
    #[pyo3(signature = (outputs=None))]
    fn trigger(&mut self, outputs: Option<Vec<u8>>) -> PyResult<()> {
        let outputs = outputs.unwrap_or_else(|| vec![0, 0]);
        self.inner
            .trigger(&outputs)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Set PWM output value (BITalino 2.0+ only).
    ///
    /// Controls the pulse-width modulation output for LED brightness
    /// or other PWM-driven devices.
    ///
    /// Args:
    ///     value: PWM duty cycle (0-255).
    ///         0 = 0% duty cycle (always off)
    ///         255 = 100% duty cycle (always on)
    ///
    /// Raises:
    ///     RuntimeError: If device is not BITalino 2.0+
    #[pyo3(signature = (value=100))]
    fn pwm(&mut self, value: u8) -> PyResult<()> {
        self.inner
            .pwm(value)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("Bitalino(rate={}Hz)", self.sampling_rate)
    }
}

/// The Python module definition
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBitalino>()?;
    m.add_class::<PyFrame>()?;
    m.add_class::<PyFrameBatch>()?;
    m.add_class::<PyDeviceState>()?;

    // Add module-level constants
    m.add("DEFAULT_SAMPLING_RATE", 1000u16)?;
    m.add("VALID_SAMPLING_RATES", vec![1u16, 10, 100, 1000])?;

    Ok(())
}
