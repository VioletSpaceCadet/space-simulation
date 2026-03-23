"""Tests for load_run module."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
import pytest

from scripts.analysis.load_run import load_run


def _write_seed_dir(
    seed_dir: Path,
    seed: int,
    ticks: list[int],
    use_parquet: bool = True,
) -> None:
    """Write minimal test data for a seed directory."""
    seed_dir.mkdir(parents=True, exist_ok=True)

    # Write run_info.json
    info: dict[str, Any] = {
        "run_id": f"seed_{seed}",
        "seed": seed,
        "content_version": "test_v1",
        "metrics_every": 10,
    }
    with open(seed_dir / "run_info.json", "w") as f:
        json.dump(info, f)

    # Build a minimal Arrow table with required columns
    table = pa.table(
        {
            "tick": pa.array(ticks, type=pa.uint64()),
            "metrics_version": pa.array([9] * len(ticks), type=pa.uint32()),
            "total_ore_kg": pa.array([100.0 + i for i in range(len(ticks))], type=pa.float32()),
            "total_material_kg": pa.array([50.0 + i for i in range(len(ticks))], type=pa.float32()),
            "total_slag_kg": pa.array([10.0] * len(ticks), type=pa.float32()),
            "station_storage_used_pct": pa.array([0.5] * len(ticks), type=pa.float32()),
            "ship_cargo_used_pct": pa.array([0.3] * len(ticks), type=pa.float32()),
            "fleet_total": pa.array([3] * len(ticks), type=pa.uint32()),
            "fleet_idle": pa.array([1] * len(ticks), type=pa.uint32()),
            "fleet_mining": pa.array([1] * len(ticks), type=pa.uint32()),
            "fleet_transiting": pa.array([1] * len(ticks), type=pa.uint32()),
            "fleet_surveying": pa.array([0] * len(ticks), type=pa.uint32()),
            "fleet_depositing": pa.array([0] * len(ticks), type=pa.uint32()),
            "refinery_active_count": pa.array([2] * len(ticks), type=pa.uint32()),
            "refinery_starved_count": pa.array([0] * len(ticks), type=pa.uint32()),
            "refinery_stalled_count": pa.array([1] * len(ticks), type=pa.uint32()),
            "power_generated_kw": pa.array([100.0] * len(ticks), type=pa.float32()),
            "power_consumed_kw": pa.array([75.0] * len(ticks), type=pa.float32()),
        }
    )

    if use_parquet:
        pq.write_table(table, seed_dir / "metrics.parquet")
    else:
        # CSV fallback — write as CSV
        import csv

        csv_path = seed_dir / "metrics_000.csv"
        with open(csv_path, "w", newline="") as f:
            writer = csv.writer(f)
            writer.writerow(table.column_names)
            for i in range(len(ticks)):
                writer.writerow([col[i].as_py() for col in table.columns])


def test_load_parquet_single_seed(tmp_path: Path) -> None:
    """Load a single seed directory with Parquet data."""
    run_dir = tmp_path / "test_run"
    _write_seed_dir(run_dir / "seed_42", seed=42, ticks=[10, 20, 30])

    rel = load_run(run_dir)
    row_count = rel.count("*").fetchone()
    assert row_count is not None
    assert row_count[0] == 3

    # Check metadata columns were added
    assert "seed" in rel.columns
    assert "content_version" in rel.columns
    assert "metrics_every" in rel.columns


def test_load_multiple_seeds(tmp_path: Path) -> None:
    """Load multiple seed directories and verify union."""
    run_dir = tmp_path / "test_run"
    _write_seed_dir(run_dir / "seed_42", seed=42, ticks=[10, 20])
    _write_seed_dir(run_dir / "seed_99", seed=99, ticks=[10, 20, 30])

    rel = load_run(run_dir)
    row_count = rel.count("*").fetchone()
    assert row_count is not None
    assert row_count[0] == 5  # 2 + 3

    # Verify distinct seeds
    seed_count = rel.aggregate("count(DISTINCT seed) AS n").fetchone()
    assert seed_count is not None
    assert seed_count[0] == 2


def test_load_csv_fallback(tmp_path: Path) -> None:
    """Fall back to CSV when Parquet is not available."""
    run_dir = tmp_path / "test_run"
    _write_seed_dir(run_dir / "seed_42", seed=42, ticks=[10, 20], use_parquet=False)

    rel = load_run(run_dir)
    row_count = rel.count("*").fetchone()
    assert row_count is not None
    assert row_count[0] == 2


def test_load_missing_dir(tmp_path: Path) -> None:
    """Raise FileNotFoundError for missing directory."""
    with pytest.raises(FileNotFoundError):
        load_run(tmp_path / "nonexistent")


def test_load_empty_dir(tmp_path: Path) -> None:
    """Raise FileNotFoundError when no seed dirs exist."""
    run_dir = tmp_path / "empty_run"
    run_dir.mkdir()
    with pytest.raises(FileNotFoundError):
        load_run(run_dir)
