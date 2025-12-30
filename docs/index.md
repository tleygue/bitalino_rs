# bitalino-rs

Rust driver with Python bindings for BITalino biosignal acquisition over Bluetooth RFCOMM.

**Transport modes**
- Default (manual RFCOMM): uses a raw Bluetooth RFCOMM socket via libc. You must pre-pair/trust the device (e.g., with `bluetoothctl`) and provide the MAC. No BlueZ runtime or tokio dependency is needed.
- Optional BlueZ mode: enable Cargo feature `bluez` to auto-discover, pair, and connect via BlueZ + tokio. Keeps the old behavior but pulls dbus/BlueZ headers and services.

## What this library does

- Opens an RFCOMM stream to a BITalino given its MAC. In BlueZ mode it can discover/pair for you; in the default manual mode you pair/trust beforehand.
- Streams frames at 1/10/100/1000 Hz with CRC checks and sequence counters to flag gaps.
- Exposes the same concepts in Rust and Python: `Bitalino`, `Frame`, `FrameBatch`, `DeviceState`.
- Provides timing hints (microsecond timestamps) so you can rebuild sample times on the host side.

## How the pieces fit

1) **Connect**: use the Bluetooth connector to open RFCOMM (manual mode) or to pair+connect (BlueZ mode).
2) **Start**: select sampling rate and channel mask; device begins streaming immediately.
3) **Read**: pull batches; inspect CRC and sequence gaps to detect drops.
4) **Stop**: stop streaming and close the transport cleanly.

## Quick start (Python)

```python
import bitalino_rs as brs

device = brs.Bitalino.connect("7E:91:2B:C4:AF:08")
device.start(rate=1000, channels=[0, 1, 2])
batch = device.read(200)
print(f"frames={len(batch)}, crc_errors={batch.crc_errors}, gaps={batch.sequence_gaps}")
device.stop()
```

## Next steps

- Install (see `installation`).
- Explore the Python API reference (`python_api`).
- Dive into driver internals in the Rust docs (`rust_api`).
