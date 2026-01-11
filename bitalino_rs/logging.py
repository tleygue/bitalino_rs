"""Logging helpers for the BITalino Rust core.

These functions bridge Rust logging into Python's ``logging`` module. They are
safe to call multiple times.
"""

from bitalino_rs._bitalino_core import enable_rust_logs, reset_log_cache

__all__ = ["enable_rust_logs", "reset_log_cache"]
