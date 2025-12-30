"""Helpers to bridge Rust logging into Python's ``logging`` module."""

def enable_rust_logs(level: str | None = ...) -> None:
    """Enable Rust-side logging bridged into Python's ``logging`` module."""

def reset_log_cache() -> None:
    """Clear cached Python logger lookups after reconfiguring logging."""

__all__ = ["enable_rust_logs", "reset_log_cache"]
