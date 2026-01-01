//! Error types surfaced by the BITalino driver, split between transport and Bluetooth concerns.
use std::io;
use thiserror::Error;

/// Top-level driver errors surfaced to callers.
#[derive(Debug, Error)]
pub enum DriverError {
    #[error("io error: {0}")]
    /// I/O failures from stdlib operations.
    Io(#[from] io::Error),
    #[error("serial error: {0}")]
    /// Serial-port layer errors.
    Serial(#[from] serialport::Error),
    #[error("bluetooth error: {0}")]
    /// Bluetooth-related issues (pairing/connectivity).
    Bluetooth(#[from] BluetoothError),
    #[error("protocol error: {0}")]
    #[allow(dead_code)]
    /// Violations of the BITalino wire protocol.
    Protocol(String),
    #[error("timeout: {0}")]
    /// Operations that exceeded their allotted time budget.
    Timeout(String),
    #[error("command failed: {0}")]
    /// Device commands that returned an error.
    Command(String),
    #[error("CRC validation failed")]
    #[allow(dead_code)]
    /// CRC check did not validate frame contents.
    Crc,
    #[error("device not ready: {0}")]
    #[allow(dead_code)]
    /// Device reported it is not ready for the requested action.
    NotReady(String),
}

/// Bluetooth-specific failures separated from transport and protocol issues.
#[derive(Debug, Error)]
pub enum BluetoothError {
    #[error("device not found during scan: {mac}")]
    /// Adapter scan failed to discover the requested MAC address.
    NotFound { mac: String },
    #[error("pairing failed: {0}")]
    /// Pairing handshake failed.
    Pairing(String),
    #[error("connection not established: {0}")]
    /// RFCOMM connection was not established.
    NotConnected(String),
    #[error("rfcomm connection failed: {0}")]
    /// Low-level RFCOMM socket errors.
    Connection(String),
}

/// Convenience result alias for driver operations.
pub type Result<T> = std::result::Result<T, DriverError>;
