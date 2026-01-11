use std::fs::File;
use std::io::{Read, Write};
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd};
use std::thread;
use std::time::Duration;

use log::{debug, info, warn};

use crate::errors::{BluetoothError, DriverError, Result};

const AF_BLUETOOTH: libc::c_ushort = 31;
const BTPROTO_RFCOMM: libc::c_int = 3;

const DEFAULT_IO_TIMEOUT_SECS: u64 = 5;
const MAX_CONNECT_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 500;

/// High level connector that opens an RFCOMM socket without needing root.
///
/// The connector expects the device to already be paired/trusted (e.g., via
/// `bluetoothctl`); you provide the MAC address and the optional PIN argument is
/// ignored. Only a minimal libc-based stack is used.
#[derive(Debug, Clone)]
pub struct BluetoothConnector {
    /// RFCOMM channel to connect to (BITalino default: 1).
    pub channel: u8,
    /// Per-operation I/O timeout applied to the RFCOMM socket.
    pub io_timeout: Duration,
    /// Maximum retry attempts for establishing RFCOMM.
    pub max_retries: u32,
    /// Delay between retries (exponential backoff uses this as the base).
    pub retry_delay: Duration,
}

impl Default for BluetoothConnector {
    fn default() -> Self {
        Self {
            channel: 1,
            io_timeout: Duration::from_secs(DEFAULT_IO_TIMEOUT_SECS),
            max_retries: MAX_CONNECT_RETRIES,
            retry_delay: Duration::from_millis(RETRY_DELAY_MS),
        }
    }
}

impl BluetoothConnector {
    /// Connect to an already-paired BITalino via RFCOMM using only libc sockets.
    /// Caller must have paired and trusted the device ahead of time (e.g., via `bluetoothctl`).
    pub fn pair_and_connect(&self, mac: &str, _pin: &str) -> Result<RfcommStream> {
        let bdaddr = parse_bdaddr(mac)?;

        let mut last_error = None;
        for attempt in 0..self.max_retries {
            if attempt > 0 {
                let delay = self.retry_delay * (1 << (attempt - 1).min(3));
                warn!(
                    "retrying RFCOMM connection after {:?} (mac={}, attempt={})",
                    delay, mac, attempt
                );
                thread::sleep(delay);
            }

            match open_rfcomm_raw(bdaddr, self.channel, self.io_timeout) {
                Ok(stream) => {
                    if let Err(e) = stream.verify_connected() {
                        warn!("connection verification failed: mac={}, error={}", mac, e);
                        last_error = Some(e);
                        continue;
                    }
                    info!("RFCOMM connection established: mac={}", mac);
                    return Ok(stream);
                }
                Err(e) => {
                    warn!(
                        "RFCOMM connection attempt failed: mac={}, attempt={}, error={}",
                        mac, attempt, e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            DriverError::Bluetooth(BluetoothError::NotConnected("max retries exceeded".into()))
        }))
    }
}

/// Simple RFCOMM stream that behaves like a Read/Write object.
pub struct RfcommStream {
    file: File,
}

impl RfcommStream {
    /// Verify the connection is actually established and usable.
    pub fn verify_connected(&self) -> Result<()> {
        // Check socket error status
        let mut err: libc::c_int = 0;
        let mut len: libc::socklen_t = mem::size_of::<libc::c_int>() as libc::socklen_t;

        let ret = unsafe {
            libc::getsockopt(
                self.file.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                &mut err as *mut _ as *mut libc::c_void,
                &mut len,
            )
        };

        if ret < 0 {
            return Err(DriverError::Io(std::io::Error::last_os_error()));
        }

        if err != 0 {
            return Err(DriverError::Bluetooth(BluetoothError::NotConnected(
                std::io::Error::from_raw_os_error(err).to_string(),
            )));
        }

        Ok(())
    }
}

impl Read for RfcommStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for RfcommStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

// Allow Send for RfcommStream (File is Send)
unsafe impl Send for RfcommStream {}

#[repr(C)]
#[derive(Copy, Clone)]
struct BdAddr {
    b: [u8; 6],
}

#[repr(C)]
struct SockAddrRc {
    rc_family: libc::sa_family_t,
    rc_bdaddr: BdAddr,
    rc_channel: u8,
}

fn parse_bdaddr(mac: &str) -> Result<BdAddr> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        return Err(DriverError::Bluetooth(BluetoothError::Connection(
            "invalid mac".into(),
        )));
    }

    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        let byte = u8::from_str_radix(part, 16).map_err(|_| {
            DriverError::Bluetooth(BluetoothError::Connection("invalid mac".into()))
        })?;
        bytes[i] = byte;
    }

    // bdaddr_t stores bytes in reverse order compared to the usual MAC string
    let mut addr = BdAddr { b: [0; 6] };
    for i in 0..6 {
        addr.b[i] = bytes[5 - i];
    }
    Ok(addr)
}

fn open_rfcomm_raw(address: BdAddr, channel: u8, timeout: Duration) -> Result<RfcommStream> {
    debug!(
        "opening RFCOMM socket: channel={}, addr_bytes={:02X?}",
        channel, address.b
    );

    let fd = unsafe {
        libc::socket(
            AF_BLUETOOTH as libc::c_int,
            libc::SOCK_STREAM,
            BTPROTO_RFCOMM,
        )
    };
    if fd < 0 {
        return Err(DriverError::Bluetooth(BluetoothError::Connection(
            std::io::Error::last_os_error().to_string(),
        )));
    }

    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        let err = std::io::Error::last_os_error();
        unsafe {
            libc::close(fd);
        }
        return Err(DriverError::Bluetooth(BluetoothError::Connection(
            err.to_string(),
        )));
    }

    let mut addr = SockAddrRc {
        rc_family: AF_BLUETOOTH as libc::sa_family_t,
        rc_bdaddr: address,
        rc_channel: channel,
    };

    let ret = unsafe {
        libc::connect(
            fd,
            &mut addr as *mut _ as *const libc::sockaddr,
            mem::size_of::<SockAddrRc>() as libc::socklen_t,
        )
    };
    if ret < 0 {
        let err = std::io::Error::last_os_error();
        unsafe {
            libc::close(fd);
        }
        return Err(DriverError::Bluetooth(BluetoothError::Connection(
            err.to_string(),
        )));
    }

    // Set IO timeouts to avoid hanging reads/writes.
    let tv = libc::timeval {
        tv_sec: timeout.as_secs() as libc::time_t,
        tv_usec: timeout.subsec_micros() as libc::suseconds_t,
    };
    for opt in [libc::SO_RCVTIMEO, libc::SO_SNDTIMEO] {
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                opt,
                &tv as *const _ as *const libc::c_void,
                mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(DriverError::Bluetooth(BluetoothError::Connection(
                err.to_string(),
            )));
        }
    }

    let file = unsafe { File::from_raw_fd(fd) };
    Ok(RfcommStream { file })
}
