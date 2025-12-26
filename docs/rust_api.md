# Rust API

The Rust documentation is generated with `cargo doc` and published alongside the site.

👉 **[Open Rust API documentation](rust-docs/bitalino_rs/index.html)**

## Notes

To regenerate locally:

```bash
uv run cargo doc --no-deps --all-features
```

The HTML will land in `target/doc/`; copy `target/doc/bitalino_rs` to `docs/rust-docs/bitalino_rs` when publishing.
