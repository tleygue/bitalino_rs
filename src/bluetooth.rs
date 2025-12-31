use std::fs::File;
use std::io::{Read, Write};
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd};
use std::time::{Duration, Instant};

use bluer::agent::{Agent, RequestConfirmationFn, RequestPinCodeFn};
use bluer::rfcomm::{SocketAddr, Stream};
use bluer::{AdapterEvent, Address, Session};
use futures::StreamExt;
use log::{debug, info, warn};
use tokio::runtime::Runtime;

use crate::errors::{BluetoothError, DriverError, Result};

const SCAN_TIMEOUT_SECS: u64 = 30;
const PAIR_TIMEOUT_SECS: u64 = 15;
const DEFAULT_IO_TIMEOUT_SECS: u64 = 5;
const MAX_CONNECT_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 500;

/// High level connector that pairs the device and opens an RFCOMM socket without needing root.
#[derive(Debug, Clone)]
pub struct BluetoothConnector {
    pub channel: u8,
    pub io_timeout: Duration,
    pub scan_timeout: Duration,
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

impl BluetoothConnector {
    /// Pair and open a stream to the sensor using an RFCOMM socket (no /dev/rfcomm required).
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
