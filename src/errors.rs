//! Error types surfaced by the BITalino driver, split between transport and Bluetooth concerns.
use std::io;
use thiserror::Error;

#[cfg_attr(not(feature = "bluez"), allow(dead_code))]
#[derive(Debug, Error)]
pub enum DriverError {
    /// I/O failures from stdlib operations.
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// Serial-port layer errors.
    #[error("serial error: {0}")]
    Serial(#[from] serialport::Error),

    /// Bluetooth-related issues (pairing/connectivity).
    #[error("bluetooth error: {0}")]
    Bluetooth(#[from] BluetoothError),

    /// Violations of the BITalino wire protocol.
    #[error("protocol error: {0}")]
    #[allow(dead_code)]
    Protocol(String),

    /// Operations that exceeded their allotted time budget.
    #[error("timeout: {0}")]
    #[allow(dead_code)]
    Timeout(String),

    /// Device commands that returned an error.
    #[error("command failed: {0}")]
    #[allow(dead_code)]
    Command(String),

    /// CRC check did not validate frame contents.
    #[error("CRC validation failed")]
    #[allow(dead_code)]
    Crc,

    /// Device reported it is not ready for the requested action.
    #[error("device not ready: {0}")]
    #[allow(dead_code)]
    NotReady(String),
}

#[cfg_attr(not(feature = "bluez"), allow(dead_code))]
#[derive(Debug, Error)]
pub enum BluetoothError {
    /// Adapter scan failed to discover the requested MAC address.
    #[error("device not found during scan: {mac}")]
    #[allow(dead_code)]
    NotFound { mac: String },

    /// Pairing handshake failed.
    #[error("pairing failed: {0}")]
    #[allow(dead_code)]
    Pairing(String),

    /// RFCOMM connection was not established.
    #[error("connection not established: {0}")]
    NotConnected(String),

    /// Low-level RFCOMM socket errors.
    #[error("rfcomm connection failed: {0}")]
    Connection(String),
}

/// Convenience result alias for driver operations.
pub type Result<T> = std::result::Result<T, DriverError>;
