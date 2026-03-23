"""Smoke tests validating the Python toolchain is correctly configured."""

import importlib
import sys


def test_python_version() -> None:
    """Verify Python >= 3.10 is in use."""
    assert sys.version_info >= (3, 10), f"Python >= 3.10 required, got {sys.version}"


def test_duckdb_importable() -> None:
    """Verify duckdb is installed and importable."""
    duckdb = importlib.import_module("duckdb")
    assert hasattr(duckdb, "connect")


def test_pyarrow_importable() -> None:
    """Verify pyarrow is installed and importable."""
    pyarrow = importlib.import_module("pyarrow")
    assert hasattr(pyarrow, "schema")


def test_pyarrow_parquet_importable() -> None:
    """Verify pyarrow.parquet submodule is available."""
    pq = importlib.import_module("pyarrow.parquet")
    assert hasattr(pq, "read_table")
