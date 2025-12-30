use std::fs::File;
use std::io::{Read, Write};
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd};
use std::time::Duration;
#[cfg(feature = "bluez")]
use std::time::Instant;

#[cfg(feature = "bluez")]
use bluer::agent::{Agent, RequestConfirmationFn, RequestPinCodeFn};
#[cfg(feature = "bluez")]
use bluer::rfcomm::{SocketAddr, Stream};
#[cfg(feature = "bluez")]
use bluer::{AdapterEvent, Address, Session};
#[cfg(feature = "bluez")]
use futures::StreamExt;
use log::{debug, info, warn};
#[cfg(feature = "bluez")]
use tokio::runtime::Runtime;

#[cfg(not(feature = "bluez"))]
use std::thread;

use crate::errors::{BluetoothError, DriverError, Result};

#[cfg(not(feature = "bluez"))]
const AF_BLUETOOTH: libc::c_ushort = 31;
#[cfg(not(feature = "bluez"))]
const BTPROTO_RFCOMM: libc::c_int = 3;

const SCAN_TIMEOUT_SECS: u64 = 30;
const PAIR_TIMEOUT_SECS: u64 = 15;
const DEFAULT_IO_TIMEOUT_SECS: u64 = 5;
const MAX_CONNECT_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 500;

/// High level connector that opens an RFCOMM socket without needing root.
///
/// Behavior depends on build features:
/// - Default (manual RFCOMM): uses raw RFCOMM and expects the device is already
///   paired/trusted (you provide the MAC).
/// - With `bluez` feature: discovers/pairs (PIN) and connects via BlueZ.
#[derive(Debug, Clone)]
pub struct BluetoothConnector {
    pub channel: u8,
    pub io_timeout: Duration,
    #[cfg_attr(not(feature = "bluez"), allow(dead_code))]
    pub scan_timeout: Duration,
    #[cfg_attr(not(feature = "bluez"), allow(dead_code))]
    pub pair_timeout: Duration,
    pub max_retries: u32,
    pub retry_delay: Duration,
}

impl Default for BluetoothConnector {
    fn default() -> Self {
        Self {
            channel: 1,
            io_timeout: Duration::from_secs(DEFAULT_IO_TIMEOUT_SECS),
            scan_timeout: Duration::from_secs(SCAN_TIMEOUT_SECS),
            pair_timeout: Duration::from_secs(PAIR_TIMEOUT_SECS),
            max_retries: MAX_CONNECT_RETRIES,
            retry_delay: Duration::from_millis(RETRY_DELAY_MS),
        }
    }
}

#[cfg(feature = "bluez")]
impl BluetoothConnector {
    /// Pair and open a stream to the sensor using an RFCOMM socket without needing root.
    ///
    /// This method includes automatic retry logic for flaky Bluetooth connections.
    pub fn pair_and_connect(&self, mac: &str, pin: &str) -> Result<RfcommStream> {
        let rt = Runtime::new()
            .map_err(|e| DriverError::Command(format!("tokio runtime init failed: {e}")))?;
        rt.block_on(self.pair_and_connect_async(mac, pin))
    }

    async fn pair_and_connect_async(&self, mac: &str, pin: &str) -> Result<RfcommStream> {
        let session = Session::new()
            .await
            .map_err(|e| DriverError::Bluetooth(BluetoothError::Connection(e.to_string())))?;
        let adapter = session
            .default_adapter()
            .await
            .map_err(|e| DriverError::Bluetooth(BluetoothError::Connection(e.to_string())))?;
        adapter
            .set_powered(true)
            .await
            .map_err(|e| DriverError::Bluetooth(BluetoothError::Connection(e.to_string())))?;

        let agent = build_agent(pin.to_string());
        let agent_handle = session
            .register_agent(agent)
            .await
            .map_err(|e| DriverError::Bluetooth(BluetoothError::Pairing(e.to_string())))?;

        let address: Address = mac.parse().map_err(|_| {
            DriverError::Bluetooth(BluetoothError::Connection("invalid mac".into()))
        })?;

        wait_for_device(&adapter, address, self.scan_timeout).await?;
        let device = adapter
            .device(address)
            .map_err(|e| DriverError::Bluetooth(BluetoothError::Connection(e.to_string())))?;

        if !device.is_paired().await.unwrap_or(false) {
            info!("pairing device via bluer: mac={}", mac);
            tokio::time::timeout(self.pair_timeout, device.pair())
                .await
                .map_err(|_| DriverError::Timeout("pairing timed out".into()))
                .and_then(|r| {
                    r.map_err(|e| DriverError::Bluetooth(BluetoothError::Pairing(e.to_string())))
                })?;
        }

        // Set device as trusted (best effort)
        let _ = device.set_trusted(true).await;

        drop(agent_handle);

        // Retry RFCOMM connection with exponential backoff
        // Note: We do NOT call device.connect() as BITalino doesn't support
        // the standard Bluetooth connect protocol. RFCOMM socket handles connection.
        let mut last_error = None;
        for attempt in 0..self.max_retries {
            if attempt > 0 {
                let delay = self.retry_delay * (1 << (attempt - 1).min(3));
                warn!(
                    "retrying RFCOMM connection after {:?} (mac={}, attempt={})",
                    delay, mac, attempt
                );
                tokio::time::sleep(delay).await;
            }

            match open_rfcomm(address, self.channel, self.io_timeout).await {
                Ok(stream) => {
                    // Verify connection is actually usable
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

#[cfg(not(feature = "bluez"))]
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
                    "retrying RFCOMM connection (manual mode) after {:?} (mac={}, attempt={})",
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
                    info!("RFCOMM connection established (manual mode): mac={}", mac);
                    return Ok(stream);
                }
                Err(e) => {
                    warn!(
                        "RFCOMM connection attempt failed (manual mode): mac={}, attempt={}, error={}",
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
    #[allow(dead_code)]
    read_timeout: Duration,
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

    /// Get the read timeout duration.
    #[allow(dead_code)]
    pub fn read_timeout(&self) -> Duration {
        self.read_timeout
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

#[cfg(feature = "bluez")]
fn build_agent(pin: String) -> Agent {
    let pin_code_fn: RequestPinCodeFn = Box::new(move |_req| {
        let pin = pin.clone();
        Box::pin(async move { Ok(pin.clone()) })
    });

    let confirm_fn: RequestConfirmationFn = Box::new(|_req| Box::pin(async { Ok(()) }));

    Agent {
        request_default: true,
        request_pin_code: Some(pin_code_fn),
        request_confirmation: Some(confirm_fn),
        ..Default::default()
    }
}

#[cfg(feature = "bluez")]
async fn wait_for_device(
    adapter: &bluer::Adapter,
    address: Address,
    timeout: Duration,
) -> Result<()> {
    let mut events = adapter
        .discover_devices()
        .await
        .map_err(|e| DriverError::Bluetooth(BluetoothError::Connection(e.to_string())))?;
    let deadline = Instant::now() + timeout;

    while let Some(evt) = events.next().await {
        match evt {
            AdapterEvent::DeviceAdded(addr) if addr == address => {
                info!("device discovered: mac={}", addr);
                return Ok(());
            }
            _ => {}
        }

        if Instant::now() > deadline {
            return Err(DriverError::Bluetooth(BluetoothError::NotFound {
                mac: address.to_string(),
            }));
        }
    }

    Err(DriverError::Bluetooth(BluetoothError::NotFound {
        mac: address.to_string(),
    }))
}

#[cfg(feature = "bluez")]
async fn open_rfcomm(address: Address, channel: u8, timeout: Duration) -> Result<RfcommStream> {
    debug!(
        "opening RFCOMM socket: mac={}, channel={}",
        address, channel
    );

    let target = SocketAddr::new(address, channel);
    let stream = tokio::time::timeout(timeout, Stream::connect(target))
        .await
        .map_err(|_| DriverError::Timeout("rfcomm connect timed out".into()))
        .and_then(|r| {
            r.map_err(|e| DriverError::Bluetooth(BluetoothError::Connection(e.to_string())))
        })?;

    // Duplicate the fd so we can make it blocking and own it separately from the async stream.
    let raw_fd = stream.as_raw_fd();
    let fd = unsafe { libc::dup(raw_fd) };
    if fd < 0 {
        let err = std::io::Error::last_os_error();
        return Err(DriverError::Bluetooth(BluetoothError::Connection(
            err.to_string(),
        )));
    }

    // Ensure the duplicated fd is cloexec to avoid leaking into child processes.
    let cloexec = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    if cloexec < 0 {
        let err = std::io::Error::last_os_error();
        unsafe {
            libc::close(fd);
        }
        return Err(DriverError::Bluetooth(BluetoothError::Connection(
            err.to_string(),
        )));
    }

    // Clear O_NONBLOCK to make blocking reads compatible with File.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        let err = std::io::Error::last_os_error();
        unsafe {
            libc::close(fd);
        }
        return Err(DriverError::Bluetooth(BluetoothError::Connection(
            err.to_string(),
        )));
    }
    let new_flags = flags & !libc::O_NONBLOCK;
    if unsafe { libc::fcntl(fd, libc::F_SETFL, new_flags) } < 0 {
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
    Ok(RfcommStream {
        file,
        read_timeout: timeout,
    })
}

#[cfg(not(feature = "bluez"))]
#[repr(C)]
#[derive(Copy, Clone)]
struct BdAddr {
    b: [u8; 6],
}

#[cfg(not(feature = "bluez"))]
#[repr(C)]
struct SockAddrRc {
    rc_family: libc::sa_family_t,
    rc_bdaddr: BdAddr,
    rc_channel: u8,
}

#[cfg(not(feature = "bluez"))]
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

#[cfg(not(feature = "bluez"))]
fn open_rfcomm_raw(address: BdAddr, channel: u8, timeout: Duration) -> Result<RfcommStream> {
    debug!(
        "opening RFCOMM socket (manual): channel={}, addr_bytes={:02X?}",
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
    Ok(RfcommStream {
        file,
        read_timeout: timeout,
    })
}
