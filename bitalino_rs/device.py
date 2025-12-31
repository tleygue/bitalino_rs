"""Device driver façade for BITalino hardware.

Thin wrapper around the compiled `_core` extension so downstream users can
import from a stable, Pythonic module path.
"""

from bitalino_rs._core import Bitalino

__all__ = ["Bitalino"]
