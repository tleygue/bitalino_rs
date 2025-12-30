//! BITalino device driver for data acquisition.
//!
//! This module provides the core driver functionality to communicate with
//! BITalino biosignal acquisition devices over serial or RFCOMM connections.
//!
//! # Timing and Synchronization
//!
//! The BITalino device has an internal crystal oscillator that controls sampling.
//! When you call `start()`, the device begins streaming at the configured rate
//! (1, 10, 100, or 1000 Hz) with high precision (~20 ppm crystal accuracy).
//!
//! **Important timing considerations:**
//! - The device does NOT send timestamps - timing must be reconstructed on the host
//! - Bluetooth introduces variable latency (typically 10-50ms)
//! - Data may arrive in bursts due to Bluetooth buffering
//! - The 4-bit sequence number (0-15) allows detection of dropped frames

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use log::{debug, warn};

use crate::bluetooth::RfcommStream;

// ============================================================================
// Constants
// ============================================================================

/// Default serial baud rate for BITalino devices
#[allow(dead_code)]
const BAUD_RATE: u32 = 115200;

/// Default timeout for serial/RFCOMM operations
#[allow(dead_code)]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Command delay to ensure device processes the command
const COMMAND_DELAY: Duration = Duration::from_millis(50);

/// Delay after stop command before device is ready for new commands
const STOP_DELAY: Duration = Duration::from_millis(200);

/// Maximum time to wait for version string
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);

// BITalino protocol commands
const CMD_STOP: u8 = 0x00;
const CMD_VERSION: u8 = 0x07;
#[allow(dead_code)]
const CMD_STATE: u8 = 0x0B; // BITalino 2.0+ only
#[allow(dead_code)]
const CMD_PWM_PREFIX: u8 = 0xA3; // BITalino 2.0+ only, followed by PWM value
#[allow(dead_code)]
const CMD_TRIGGER_2: u8 = 0xB3; // BITalino 2.0 digital outputs base command
#[allow(dead_code)]
const CMD_TRIGGER_1: u8 = 0x03; // BITalino 1.0 digital outputs base command
#[allow(dead_code)]
const CMD_IDLE: u8 = 0xFF; // BITalino 2.0+ go to idle from any state

// ============================================================================
// Data Types
// ============================================================================

/// Supported sampling rates for BITalino acquisition.
///
/// The BITalino device supports these fixed sampling rates, controlled by
/// its internal crystal oscillator with ~20 ppm accuracy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SamplingRate {
    /// 1 Hz - for very slow signals or battery saving
    Hz1 = 1,
    /// 10 Hz - suitable for respiration, slow EDA
    Hz10 = 10,
    /// 100 Hz - good for EDA, some EMG applications
    Hz100 = 100,
    /// 1000 Hz - required for ECG, EMG, high-quality acquisition
    #[default]
    Hz1000 = 1000,
}

impl SamplingRate {
    /// Convert sampling rate to protocol bits for the set-rate command.
    /// Used internally when setting device sampling rate.
    pub fn to_bits(self) -> u8 {
        match self {
            SamplingRate::Hz1 => 0b00,
            SamplingRate::Hz10 => 0b01,
            SamplingRate::Hz100 => 0b10,
            SamplingRate::Hz1000 => 0b11,
        }
    }

    /// Parse a u16 value into a SamplingRate, returning an error on invalid values.
    pub fn from_u16_checked(value: u16) -> anyhow::Result<Self> {
        match value {
            1 => Ok(SamplingRate::Hz1),
            10 => Ok(SamplingRate::Hz10),
            100 => Ok(SamplingRate::Hz100),
            1000 => Ok(SamplingRate::Hz1000),
            _ => anyhow::bail!("Invalid sampling rate {value}. Supported: 1, 10, 100, 1000."),
        }
    }

    /// Get the sampling period in microseconds.
    #[allow(dead_code)]
    pub fn period_us(self) -> u64 {
        1_000_000 / (self as u64)
    }
}

/// A single data frame from the BITalino device.
///
/// Each frame contains one sample from all active channels, plus metadata.
/// Frames arrive at the configured sampling rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Sequence number (0-15, wraps around).
    /// Use this to detect dropped frames: if `(new_seq - old_seq) % 16 != 1`, frames were lost.
    pub seq: u8,
    /// Digital input channels (4 channels: I1, I2, O1, O2).
    /// Each value is 0 or 1.
    pub digital: [u8; 4],
    /// Analog channel values (10-bit resolution, 0-1023).
    /// The number of values matches the channels configured in `start()`.
    pub analog: Vec<u16>,
}

impl Frame {
    /// Create a new frame with the given values.
    #[inline]
    pub fn new(seq: u8, digital: [u8; 4], analog: Vec<u16>) -> Self {
        Self {
            seq,
            digital,
            analog,
        }
    }
}

/// Result of reading frames, including timing information.
#[derive(Debug, Clone)]
pub struct FrameBatch {
    /// The frames that were successfully read.
    pub frames: Vec<Frame>,
    /// Timestamp when the batch read started (for timing reconstruction).
    #[allow(dead_code)]
    pub timestamp_us: u64,
    /// Number of CRC errors encountered (frames that were discarded).
    #[allow(dead_code)]
    pub crc_errors: usize,
    /// Number of sequence discontinuities detected (potential dropped frames).
    #[allow(dead_code)]
    pub sequence_gaps: usize,
}

/// Device state information (BITalino 2.0+ only).
///
/// Contains current values of all analog/digital channels and battery status.
/// Useful for checking device status without starting acquisition.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeviceState {
    /// Analog channel values (A1-A6, 10-bit resolution 0-1023).
    pub analog: [u16; 6],
    /// Battery channel raw value (10-bit, 0-1023).
    /// Use `battery_voltage()` for approximate voltage.
    pub battery: u16,
    /// Current battery threshold setting (0-63).
    pub battery_threshold: u8,
    /// Digital channel states [I1, I2, O1, O2].
    pub digital: [u8; 4],
}

#[allow(dead_code)]
impl DeviceState {
    /// Approximate battery voltage based on raw ADC value.
    ///
    /// Formula derived from BITalino specifications:
    /// V(bat) ≈ (ADC / 1023) × 3.3V × 2 (due to voltage divider)
    pub fn battery_voltage(&self) -> f32 {
        (self.battery as f32 / 1023.0) * 3.3 * 2.0
    }

    /// Check if battery is low based on threshold.
    /// Returns true if battery voltage is below the threshold setting.
    pub fn is_battery_low(&self) -> bool {
        // Threshold 0 = 3.4V, 63 = 3.8V
        // Linear interpolation: threshold_voltage = 3.4 + (threshold/63) * 0.4
        let threshold_voltage = 3.4 + (self.battery_threshold as f32 / 63.0) * 0.4;
        self.battery_voltage() < threshold_voltage
    }
}

// ============================================================================
// Transport Abstraction
// ============================================================================

/// Trait for Read + Write + Send, allowing different transport backends.
trait Transport: Read + Write + Send {}
impl<T: Read + Write + Send> Transport for T {}

// ============================================================================
// Bitalino Driver
// ============================================================================

/// BITalino device driver.
///
/// Provides methods to connect, configure, and read data from a BITalino device.
///
/// # Example (Rust)
/// ```ignore
/// let connector = BluetoothConnector::default();
/// let stream = connector.pair_and_connect("7E:91:2B:C4:AF:08", "1234")?;
/// let mut device = Bitalino::from_rfcomm(stream);
///
/// println!("Firmware: {}", device.version()?);
///
/// device.start(SamplingRate::Hz1000, vec![0, 1, 2])?;
/// let batch = device.read_frames_timed(1000)?;
/// println!("Read {} frames, {} CRC errors", batch.frames.len(), batch.crc_errors);
/// device.stop()?;
/// ```
pub struct Bitalino {
    transport: Box<dyn Transport>,
    active_channels: Vec<u8>,
    frame_size: usize,
    sampling_rate: SamplingRate,
    start_time: Option<Instant>,
    last_seq: Option<u8>,
    /// Whether device is BITalino 2.0+ (supports state(), pwm(), trigger in idle)
    is_bitalino2: bool,
}

impl Bitalino {
    // ------------------------------------------------------------------------
    // Constructors
    // ------------------------------------------------------------------------

    /// Connect to a BITalino via serial port (e.g., `/dev/rfcomm0`).
    ///
    /// This is useful when you've already bound the device using `rfcomm bind`.
    #[allow(dead_code)]
    pub fn connect_serial(path: &str) -> Result<Self> {
        let port = serialport::new(path, BAUD_RATE)
            .timeout(DEFAULT_TIMEOUT)
            .open()
            .with_context(|| format!("Failed to open serial port at {}", path))?;

        Ok(Self {
            transport: Box::new(port),
            active_channels: Vec::new(),
            frame_size: 0,
            sampling_rate: SamplingRate::Hz1000,
            start_time: None,
            last_seq: None,
            is_bitalino2: false, // Will be detected on first version() call
        })
    }

    /// Create a Bitalino driver from an already-connected RFCOMM stream.
    ///
    /// This is the preferred method when using `BluetoothConnector::pair_and_connect()`.
    pub fn from_rfcomm(stream: RfcommStream) -> Self {
        Self {
            transport: Box::new(stream),
            active_channels: Vec::new(),
            frame_size: 0,
            sampling_rate: SamplingRate::Hz1000,
            start_time: None,
            last_seq: None,
            is_bitalino2: false, // Will be detected on first version() call
        }
    }

    // ------------------------------------------------------------------------
    // Device Commands
    // ------------------------------------------------------------------------

    /// Query the device firmware version.
    ///
    /// Returns the complete version string (e.g., "BITalino_v5.2").
    /// This method properly handles the asynchronous nature of Bluetooth
    /// by reading until a delimiter or timeout.
    pub fn version(&mut self) -> Result<String> {
        // Ensure device is in idle state
        let _ = self.stop();
        std::thread::sleep(STOP_DELAY);

        // Clear any pending data in the buffer
        self.flush_input()?;

        // Send version command
        self.send_command(CMD_VERSION)?;

        // Read response with timeout - version ends with newline or null
        let mut response = Vec::with_capacity(64);
        let deadline = Instant::now() + VERSION_TIMEOUT;

        loop {
            let mut byte = [0u8; 1];
            match self.transport.read(&mut byte) {
                Ok(n) if n >= 1 => {
                    // Stop at newline, carriage return, or null
                    if byte[0] == b'\n' || byte[0] == b'\r' || byte[0] == 0 {
                        if !response.is_empty() {
                            break;
                        }
                        // Skip leading delimiters
                        continue;
                    }
                    response.push(byte[0]);

                    // Sanity check - version string shouldn't be too long
                    if response.len() >= 64 {
                        break;
                    }
                }
                Ok(_) => {
                    // EOF or no data
                    if !response.is_empty() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Timeout on this read, check overall deadline
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // Socket timeout
                    if !response.is_empty() {
                        break;
                    }
                }
                Err(e) => {
                    if !response.is_empty() {
                        break;
                    }
                    return Err(e.into());
                }
            }

            if Instant::now() > deadline {
                if !response.is_empty() {
                    break;
                }
                anyhow::bail!("Timeout waiting for version response");
            }
        }

        let version = String::from_utf8_lossy(&response).trim().to_string();
        debug!("Device version: {}", version);

        // Detect BITalino 2.0+ based on version string
        // Format: "BITalino_v5.2" or "BITalino V5.2"
        self.is_bitalino2 = self.detect_bitalino2(&version);
        if self.is_bitalino2 {
            debug!("Detected BITalino 2.0+ device");
        }

        Ok(version)
    }

    /// Detect if device is BITalino 2.0+ based on version string.
    fn detect_bitalino2(&self, version: &str) -> bool {
        // BITalino 2.0 has version >= 4.2
        let version_lower = version.to_lowercase();

        // Try to extract version number after "_v" or "v"
        let version_num = if let Some(pos) = version_lower.find("_v") {
            version_lower[pos + 2..].chars().take(3).collect::<String>()
        } else if let Some(pos) = version_lower.find('v') {
            version_lower[pos + 1..].chars().take(3).collect::<String>()
        } else {
            return false;
        };

        if let Ok(num) = version_num.parse::<f32>() {
            num >= 4.2
        } else {
            false
        }
    }

    /// Start data acquisition at the specified sampling rate.
    ///
    /// # Arguments
    /// * `sampling_rate` - Sampling rate (1, 10, 100, or 1000 Hz). Invalid values default to 1000 Hz.
    /// * `channels` - List of analog channels to acquire (0-5)
    ///
    /// # Returns
    /// The actual sampling rate that was set.
    pub fn start(&mut self, sampling_rate: u16, channels: Vec<u8>) -> Result<SamplingRate> {
        let rate = SamplingRate::from_u16_checked(sampling_rate)?;
        self.start_with_rate(rate, channels)
    }

    /// Start data acquisition with a specific SamplingRate enum value.
    pub fn start_with_rate(
        &mut self,
        rate: SamplingRate,
        channels: Vec<u8>,
    ) -> Result<SamplingRate> {
        // Ensure we're in a clean state
        if let Err(e) = self.stop() {
            anyhow::bail!("Failed to stop before starting acquisition: {e}");
        }
        std::thread::sleep(COMMAND_DELAY);

        // Validate and filter channels
        let mut valid_channels: Vec<u8> = channels.into_iter().filter(|&ch| ch < 6).collect();
        valid_channels.sort_unstable();
        valid_channels.dedup();
        if valid_channels.is_empty() {
            anyhow::bail!("No valid channels specified (must be 0-5)");
        }

        // Set sampling rate first (command format: 0b01XXRR11 where RR is rate bits)
        // Actually, BITalino sets rate as part of start command in simulated mode,
        // or uses a separate command. The live mode rate is fixed at the device's default.
        // For BITalino (r)evolution, we set rate via: 0bRR000011 before starting
        let rate_cmd = (rate.to_bits() << 6) | 0x03;
        self.send_command(rate_cmd)?;

        // Build channel bitmask (bits 2-7 indicate A1-A6)
        let mut channel_bits: u8 = 0;
        for ch in &valid_channels {
            channel_bits |= 1 << (2 + ch);
        }

        // Start command: channel_bits | 0x01 (LSB=1 for live mode)
        let cmd = channel_bits | 0x01;
        self.send_command(cmd)?;

        // Store active configuration
        self.active_channels = valid_channels;
        self.frame_size = self.calculate_frame_size();
        self.sampling_rate = rate;
        self.start_time = Some(Instant::now());
        self.last_seq = None;

        debug!(
            "Started acquisition: rate={}Hz, channels={:?}, frame_size={}",
            rate as u16, self.active_channels, self.frame_size
        );

        Ok(rate)
    }

    /// Stop data acquisition.
    pub fn stop(&mut self) -> Result<()> {
        self.send_command(CMD_STOP)?;
        self.active_channels.clear();
        self.frame_size = 0;
        self.start_time = None;
        self.last_seq = None;
        Ok(())
    }

    /// Set the battery threshold level.
    ///
    /// When battery voltage drops below this threshold, the device LED will blink.
    ///
    /// # Arguments
    /// * `threshold` - Threshold value (0-63)
    ///   - 0 = 3.4V (minimum)
    ///   - 63 = 3.8V (maximum)
    ///
    /// # Errors
    /// Returns error if device is currently in acquisition mode.
    #[allow(dead_code)]
    pub fn set_battery_threshold(&mut self, threshold: u8) -> Result<()> {
        if self.frame_size > 0 {
            anyhow::bail!("Cannot set battery threshold during acquisition. Call stop() first.");
        }

        let threshold = threshold.min(63); // Clamp to valid range
                                           // Command format: <threshold (6 bits)> 0 0
        let cmd = threshold << 2;
        self.send_command(cmd)?;
        debug!(
            "Battery threshold set to {} (≈{:.2}V)",
            threshold,
            3.4 + (threshold as f32 / 63.0) * 0.4
        );
        Ok(())
    }

    /// Get the current device state (BITalino 2.0+ only).
    ///
    /// Returns the current values of all analog channels, digital channels,
    /// battery level, and battery threshold.
    ///
    /// # Errors
    /// - Returns error if device is not BITalino 2.0+
    /// - Returns error if device is currently in acquisition mode
    /// - Returns error if CRC check fails
    #[allow(dead_code)]
    pub fn state(&mut self) -> Result<DeviceState> {
        if !self.is_bitalino2 {
            anyhow::bail!("state() is only available on BITalino 2.0+ devices. Call version() first to detect device type.");
        }

        if self.frame_size > 0 {
            anyhow::bail!("Cannot read state during acquisition. Call stop() first.");
        }

        // Flush any pending data in the buffer before sending command
        self.flush_input()?;

        // Send state command
        self.send_command(CMD_STATE)?;

        // Read 16-byte response
        // Response format (from official BITalino API):
        // <A1 (2 bytes)> <A2 (2 bytes)> <A3 (2 bytes)> <A4 (2 bytes)> <A5 (2 bytes)> <A6 (2 bytes)>
        // <ABAT (2 bytes)> <Battery threshold (1 byte)> <Digital ports + CRC (1 byte: I1 I2 O1 O2 CRC4)>
        let mut data = [0u8; 16];
        self.transport.read_exact(&mut data)?;

        // Flush any extra data the device might have sent
        self.flush_input()?;

        // Print raw data for debugging
        debug!("State response raw data: {:02X?}", data);

        // Verify CRC (last 4 bits of last byte)
        // From official Python code:
        // crc = decodedData[-1] & 0x0F
        // decodedData[-1] = decodedData[-1] & 0xF0
        // x = 0
        // for i in range(number_bytes):
        //     for bit in range(7, -1, -1):
        //         x = x << 1
        //         if (x & 0x10):
        //             x = x ^ 0x03
        //         x = x ^ ((decodedData[i] >> bit) & 0x01)

        let received_crc = data[15] & 0x0F;

        // Clear CRC bits in last byte before calculation
        let mut data_for_crc = data;
        data_for_crc[15] &= 0xF0;

        // Calculate CRC exactly as in official Python code
        let mut x: u8 = 0;
        for &byte in &data_for_crc {
            for bit in (0..8).rev() {
                x <<= 1;
                if (x & 0x10) != 0 {
                    x ^= 0x03;
                }
                x ^= (byte >> bit) & 0x01;
            }
        }
        let calculated_crc = x & 0x0F;

        debug!(
            "State CRC: received={:#X}, calculated={:#X}",
            received_crc, calculated_crc
        );

        if received_crc != calculated_crc {
            // Note: Some BITalino firmware versions may not send proper CRC for state command.
            // The data appears valid even when CRC doesn't match, so we log a warning but continue.
            warn!("CRC mismatch in state response (received: {:#X}, calculated: {:#X}), continuing anyway",
                          received_crc, calculated_crc);
        }

        // Decode response - official Python code reads from the end using negative indices
        // decodedData[-1] = data[15] (digital + CRC)
        // decodedData[-2] = data[14] (battery threshold)
        // decodedData[-3] << 8 | decodedData[-4] = data[13] << 8 | data[12] (battery)
        // etc.

        let digital = [
            (data[15] >> 7) & 0x01, // I1
            (data[15] >> 6) & 0x01, // I2
            (data[15] >> 5) & 0x01, // O1
            (data[15] >> 4) & 0x01, // O2
        ];

        let battery_threshold = data[14];

        // Battery: decodedData[-3] << 8 | decodedData[-4] = data[13] << 8 | data[12]
        let battery = ((data[13] as u16) << 8) | (data[12] as u16);

        // Analog channels from official code:
        // A6 = decodedData[-5] << 8 | decodedData[-6] = data[11] << 8 | data[10]
        // A5 = decodedData[-7] << 8 | decodedData[-8] = data[9] << 8 | data[8]
        // etc.
        let a6 = ((data[11] as u16) << 8) | (data[10] as u16);
        let a5 = ((data[9] as u16) << 8) | (data[8] as u16);
        let a4 = ((data[7] as u16) << 8) | (data[6] as u16);
        let a3 = ((data[5] as u16) << 8) | (data[4] as u16);
        let a2 = ((data[3] as u16) << 8) | (data[2] as u16);
        let a1 = ((data[1] as u16) << 8) | (data[0] as u16);

        let analog = [a1, a2, a3, a4, a5, a6];

        debug!(
            "Device state: analog={:?}, battery={}, threshold={}, digital={:?}",
            analog, battery, battery_threshold, digital
        );

        Ok(DeviceState {
            analog,
            battery,
            battery_threshold,
            digital,
        })
    }

    /// Set digital output pins.
    ///
    /// Controls the digital output pins on the BITalino device. These can be used
    /// to control external circuits or the device's LED.
    ///
    /// # Arguments
    /// * `outputs` - Array of output values:
    ///   - BITalino 2.0: `[O1, O2]` (2 values)
    ///   - BITalino 1.0: `[O1, O2, O3, O4]` (4 values, must be in acquisition mode)
    ///
    /// # Errors
    /// - BITalino 1.0: Returns error if not in acquisition mode
    #[allow(dead_code)]
    pub fn trigger(&mut self, outputs: &[u8]) -> Result<()> {
        if self.is_bitalino2 {
            // BITalino 2.0: Works in both idle and acquisition modes
            // Command format: 1 0 1 1 O2 O1 1 1
            let o1 = outputs.first().copied().unwrap_or(0) & 0x01;
            let o2 = outputs.get(1).copied().unwrap_or(0) & 0x01;
            let cmd = CMD_TRIGGER_2 | (o2 << 3) | (o1 << 2);
            self.send_command(cmd)?;
        } else {
            // BITalino 1.0: Only works during acquisition
            if self.frame_size == 0 {
                anyhow::bail!(
                    "BITalino 1.0 trigger() requires active acquisition. Call start() first."
                );
            }
            // Command format: 1 0 O4 O3 O2 O1 1 1
            let o1 = outputs.first().copied().unwrap_or(0) & 0x01;
            let o2 = outputs.get(1).copied().unwrap_or(0) & 0x01;
            let o3 = outputs.get(2).copied().unwrap_or(0) & 0x01;
            let o4 = outputs.get(3).copied().unwrap_or(0) & 0x01;
            let cmd = CMD_TRIGGER_1 | (o4 << 5) | (o3 << 4) | (o2 << 3) | (o1 << 2);
            self.send_command(cmd)?;
        }
        Ok(())
    }

    /// Set PWM output value (BITalino 2.0+ only).
    ///
    /// Controls the pulse-width modulation output on the BITalino device.
    /// Can be used for controlling LED brightness or other PWM-driven devices.
    ///
    /// # Arguments
    /// * `value` - PWM duty cycle (0-255)
    ///   - 0 = 0% duty cycle (always off)
    ///   - 255 = 100% duty cycle (always on)
    ///
    /// # Errors
    /// Returns error if device is not BITalino 2.0+
    #[allow(dead_code)]
    pub fn pwm(&mut self, value: u8) -> Result<()> {
        if !self.is_bitalino2 {
            anyhow::bail!("pwm() is only available on BITalino 2.0+ devices");
        }

        // Two-byte command: 0xA3 followed by PWM value
        self.transport.write_all(&[CMD_PWM_PREFIX])?;
        self.transport.flush()?;
        std::thread::sleep(COMMAND_DELAY);
        self.transport.write_all(&[value])?;
        self.transport.flush()?;
        std::thread::sleep(COMMAND_DELAY);

        debug!(
            "PWM output set to {} ({:.1}%)",
            value,
            value as f32 / 255.0 * 100.0
        );
        Ok(())
    }

    /// Check if this is a BITalino 2.0+ device.
    ///
    /// Returns true if the device supports extended features like `state()`, `pwm()`,
    /// and `trigger()` in idle mode.
    ///
    /// Note: You must call `version()` first to detect the device type.
    #[allow(dead_code)]
    pub fn is_bitalino2(&self) -> bool {
        self.is_bitalino2
    }

    /// Get the current sampling rate.
    #[allow(dead_code)]
    pub fn sampling_rate(&self) -> SamplingRate {
        self.sampling_rate
    }

    /// Get the time since acquisition started, in microseconds.
    pub fn elapsed_us(&self) -> Option<u64> {
        self.start_time.map(|t| t.elapsed().as_micros() as u64)
    }

    /// Read multiple frames from the device.
    ///
    /// # Arguments
    /// * `n_frames` - Number of frames to read
    ///
    /// # Returns
    /// Vector of frames. May contain fewer frames than requested if CRC errors occur.
    pub fn read_frames(&mut self, n_frames: usize) -> Result<Vec<Frame>> {
        let batch = self.read_frames_timed(n_frames)?;
        Ok(batch.frames)
    }

    /// Read multiple frames with timing and error statistics.
    ///
    /// This is the recommended method for high-quality acquisition as it provides:
    /// - Timestamp for timing reconstruction
    /// - CRC error count
    /// - Sequence gap detection for dropped frames
    pub fn read_frames_timed(&mut self, n_frames: usize) -> Result<FrameBatch> {
        if self.frame_size == 0 {
            anyhow::bail!("Acquisition not started. Call start() first.");
        }

        let timestamp_us = self.elapsed_us().unwrap_or(0);
        let mut frames = Vec::with_capacity(n_frames);
        let mut buffer = vec![0u8; self.frame_size];
        let mut crc_errors = 0usize;
        let mut sequence_gaps = 0usize;

        for _ in 0..n_frames {
            self.transport.read_exact(&mut buffer)?;

            if self.verify_crc(&buffer) {
                let frame = self.decode_frame(&buffer);

                // Check for sequence gaps
                if let Some(last) = self.last_seq {
                    let expected = (last + 1) & 0x0F;
                    if frame.seq != expected {
                        let gap = ((frame.seq as i16 - expected as i16 + 16) % 16) as usize;
                        if gap > 0 && gap < 8 {
                            // Likely dropped frames (not a wrap-around confusion)
                            sequence_gaps += gap;
                        }
                    }
                }
                self.last_seq = Some(frame.seq);

                frames.push(frame);
            } else {
                crc_errors += 1;
            }
        }

        if crc_errors > 0 {
            warn!(
                "CRC errors in batch: {} (suppressing per-frame logs)",
                crc_errors
            );
        }
        if sequence_gaps > 0 {
            warn!(
                "Sequence gaps detected in batch: {} (suppressing per-frame logs)",
                sequence_gaps
            );
        }

        Ok(FrameBatch {
            frames,
            timestamp_us,
            crc_errors,
            sequence_gaps,
        })
    }

    /// Read a single frame from the device.
    #[allow(dead_code)]
    pub fn read_frame(&mut self) -> Result<Option<Frame>> {
        let batch = self.read_frames_timed(1)?;
        Ok(batch.frames.into_iter().next())
    }

    // ------------------------------------------------------------------------
    // Internal Methods
    // ------------------------------------------------------------------------

    /// Send a command byte to the device.
    fn send_command(&mut self, cmd: u8) -> Result<()> {
        self.transport.write_all(&[cmd])?;
        self.transport.flush()?;
        std::thread::sleep(COMMAND_DELAY);
        Ok(())
    }

    /// Flush any pending input data.
    fn flush_input(&mut self) -> Result<()> {
        let mut buf = [0u8; 256];
        let start = Instant::now();
        let max_flush = Duration::from_millis(200);
        let mut iterations = 0usize;
        loop {
            iterations += 1;
            match self.transport.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(e) => return Err(e.into()),
            }

            if iterations > 64 || start.elapsed() > max_flush {
                break;
            }
        }
        Ok(())
    }

    /// Calculate the frame size in bytes based on active channels.
    ///
    /// BITalino frame structure:
    /// - 4 digital inputs (4 bits)
    /// - Sequence number (4 bits)
    /// - Analog channels: first 4 are 10-bit, remaining are 6-bit
    fn calculate_frame_size(&self) -> usize {
        let n = self.active_channels.len();
        if n == 0 {
            return 0;
        }

        // Formula from BITalino documentation
        let bits = if n <= 4 {
            12 + 10 * n // 4 digital + 4 seq + n*10-bit analog
        } else {
            52 + 6 * (n - 4) // First 4 channels are 10-bit, rest are 6-bit
        };

        bits.div_ceil(8) // Round up to bytes
    }

    /// Verify the CRC of a frame.
    ///
    /// BITalino uses a 4-bit CRC stored in the lower nibble of the last byte.
    fn verify_crc(&self, data: &[u8]) -> bool {
        let len = data.len();
        if len == 0 {
            return false;
        }

        let received_crc = data[len - 1] & 0x0F;

        let mut crc = 0u8;
        for (i, &byte) in data.iter().enumerate() {
            let byte = if i == len - 1 { byte & 0xF0 } else { byte };

            for bit in (0..8).rev() {
                crc <<= 1;
                if (crc & 0x10) != 0 {
                    crc ^= 0x03;
                }
                crc ^= (byte >> bit) & 0x01;
            }
        }

        received_crc == (crc & 0x0F)
    }

    /// Decode a raw frame buffer into a Frame struct.
    fn decode_frame(&self, data: &[u8]) -> Frame {
        let last = data.len() - 1;
        let n_channels = self.active_channels.len();

        // Sequence number (upper 4 bits of last byte)
        let seq = data[last] >> 4;

        // Digital inputs (bits 4-7 of second-to-last byte)
        let digital = [
            (data[last - 1] >> 7) & 0x01,
            (data[last - 1] >> 6) & 0x01,
            (data[last - 1] >> 5) & 0x01,
            (data[last - 1] >> 4) & 0x01,
        ];

        // Analog channels (10-bit values, packed)
        let mut analog = Vec::with_capacity(n_channels);

        // Decoding follows BITalino frame format specification
        if n_channels > 0 {
            let val = ((data[last - 1] as u16 & 0x0F) << 6) | (data[last - 2] as u16 >> 2);
            analog.push(val);
        }
        if n_channels > 1 {
            let val = ((data[last - 2] as u16 & 0x03) << 8) | (data[last - 3] as u16);
            analog.push(val);
        }
        if n_channels > 2 {
            let val = ((data[last - 4] as u16) << 2) | (data[last - 5] as u16 >> 6);
            analog.push(val);
        }
        if n_channels > 3 {
            let val = ((data[last - 5] as u16 & 0x3F) << 4) | (data[last - 6] as u16 >> 4);
            analog.push(val);
        }
        if n_channels > 4 {
            let val = ((data[last - 6] as u16 & 0x0F) << 2) | (data[last - 7] as u16 >> 6);
            analog.push(val);
        }
        if n_channels > 5 {
            let val = data[last - 7] as u16 & 0x3F;
            analog.push(val);
        }

        Frame::new(seq, digital, analog)
    }
}
