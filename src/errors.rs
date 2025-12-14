use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("serial error: {0}")]
    Serial(#[from] serialport::Error),
    #[error("bluetooth error: {0}")]
    Bluetooth(#[from] BluetoothError),
    #[error("protocol error: {0}")]
    #[allow(dead_code)]
    Protocol(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("command failed: {0}")]
    Command(String),
    #[error("CRC validation failed")]
    #[allow(dead_code)]
    Crc,
    #[error("device not ready: {0}")]
    #[allow(dead_code)]
    NotReady(String),
}

#[derive(Debug, Error)]
pub enum BluetoothError {
    #[error("device not found during scan: {mac}")]
    NotFound { mac: String },
    #[error("pairing failed: {0}")]
    Pairing(String),
    #[error("connection not established: {0}")]
    NotConnected(String),
    #[error("rfcomm connection failed: {0}")]
    Connection(String),
}

pub type Result<T> = std::result::Result<T, DriverError>;
