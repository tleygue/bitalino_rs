# bitalino-rs

Rust driver with Python bindings for BITalino biosignal acquisition over Bluetooth RFCOMM.

**Transport**
- Uses a raw Bluetooth RFCOMM socket via libc. You must pre-pair/trust the device (e.g., with `bluetoothctl`) and provide the MAC. No BlueZ runtime, tokio, or dbus dependency is needed.

## What this library does

- Opens an RFCOMM stream to a BITalino given its MAC (you pair/trust the device beforehand).
- Streams frames at 1/10/100/1000 Hz with CRC checks and sequence counters to flag gaps.
- Exposes the same concepts in Rust and Python: `Bitalino`, `Frame`, `FrameBatch`, `DeviceState`.
- Provides timing hints (microsecond timestamps) so you can rebuild sample times on the host side.

## How the pieces fit

1) **Connect**: use the Bluetooth connector to open RFCOMM (paired/trusted device).
2) **Start**: select sampling rate and channel mask; device begins streaming immediately.
3) **Read**: pull batches; inspect CRC and sequence gaps to detect drops.
4) **Stop**: stop streaming and close the transport cleanly.

## Quick start (Python)

```python
import bitalino_rs as brs

device = brs.Bitalino.connect("7E:91:2B:C4:AF:08")
device.start(rate=1000, channels=[0, 1, 2])
device.wait_until_streaming(timeout=2.0)  # block until BT link is reliable
batch = device.read_timed(200)
print(f"frames={len(batch.frames)}, crc_errors={batch.crc_errors}, gaps={batch.sequence_gaps}")
device.stop()
```

## Next steps

- Install (see `installation`).
- Explore the Python API reference (`python_api`).
- Dive into driver internals in the Rust docs (`rust_api`).
