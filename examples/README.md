# Examples

Practical, runnable scripts to get started with `bitalino_rs`. All scripts accept a `--mac` Bluetooth address (unless noted) and assume a BITalino device is powered on. With the default manual RFCOMM backend, make sure the device is already paired/trusted (e.g., via `bluetoothctl`).

```
python examples/quickstart_read.py --mac AA:BB:CC:DD:EE:FF
```

## Scripts

1. `quickstart_read.py` — Connect, start at 1000 Hz, read 100 frames, stop; prints a sample frame.
2. `timed_batch_and_gaps.py` — Use `read_timed` to inspect CRC errors and sequence gaps and reconstruct per-sample timestamps.
3. `battery_and_state.py` — Query firmware, check BITalino 2.0+, fetch `state()` and print battery info.
7. `logging_bridge.py` — Bridge Rust logs into Python’s logging and capture a short read.

Additional:
- `stream_to_csv.py` — Continuously read timed batches and append to CSV with timestamps and integrity columns.
- `plot_realtime.py` — Optional (needs `matplotlib`): live-plot a channel while reading timed batches.
- `reconnect_on_failure.py` — Demonstrates exponential-backoff reconnect loop on connection failure.

## Conventions
- Use `--mac` for Bluetooth; `--path` variants allow serial paths (e.g., `/dev/rfcomm0`).
- Scripts exit non-zero on failure; wrap in your own supervisor as needed.
- BITalino 2.0+ features (`state`, `pwm`, idle `trigger`) are guarded.
- Keep runs short; adjust `--frames` / `--duration` flags for longer sessions.
