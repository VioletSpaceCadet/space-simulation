"""Load sim_bench run output into DuckDB for analysis.

Usage:
    python3 scripts/analysis/load_run.py <run_dir>

The run directory should contain seed subdirectories (seed_*/) each with
metrics.parquet (preferred) or metrics_*.csv files plus run_info.json.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import duckdb


def load_run(run_dir: str | Path) -> duckdb.DuckDBPyRelation:
    """Load all seeds from a sim_bench run directory into a single DuckDB relation.

    Each seed's metrics are loaded from Parquet (preferred) or CSV (fallback),
    then augmented with metadata columns: seed, content_version, metrics_every.

    Args:
        run_dir: Path to the sim_bench run output directory.

    Returns:
        DuckDB relation with all seeds' metrics joined with run metadata.

    Raises:
        FileNotFoundError: If run_dir doesn't exist or contains no seed dirs.
        ValueError: If no metrics files found in any seed directory.
    """
    run_path = Path(run_dir)
    if not run_path.is_dir():
        msg = f"Run directory not found: {run_path}"
        raise FileNotFoundError(msg)

    seed_dirs = sorted(run_path.glob("seed_*"))
    if not seed_dirs:
        msg = f"No seed directories (seed_*/) found in {run_path}"
        raise FileNotFoundError(msg)

    conn = duckdb.connect(":memory:")
    relations: list[duckdb.DuckDBPyRelation] = []

    for seed_dir in seed_dirs:
        rel = _load_seed(conn, seed_dir)
        if rel is not None:
            relations.append(rel)

    if not relations:
        msg = f"No metrics files found in any seed directory under {run_path}"
        raise ValueError(msg)

    # Union all seeds into a single relation
    result = relations[0]
    for rel in relations[1:]:
        result = result.union(rel)

    return result


def _load_seed(conn: duckdb.DuckDBPyConnection, seed_dir: Path) -> duckdb.DuckDBPyRelation | None:
    """Load a single seed directory into a DuckDB relation with metadata columns."""
    # Load metadata from run_info.json
    info_path = seed_dir / "run_info.json"
    seed: int = 0
    content_version: str = "unknown"
    metrics_every: int = 1

    if info_path.exists():
        with open(info_path) as f:
            info = json.load(f)
        seed = int(info.get("seed", 0))
        content_version = str(info.get("content_version", "unknown"))
        metrics_every = int(info.get("metrics_every", 1))

    # Try Parquet first, fall back to CSV
    parquet_path = seed_dir / "metrics.parquet"
    csv_files = sorted(seed_dir.glob("metrics_*.csv"))

    if parquet_path.exists():
        rel = conn.read_parquet(str(parquet_path))
    elif csv_files:
        csv_paths = [str(p) for p in csv_files]
        rel = conn.read_csv(csv_paths)
    else:
        return None

    # Add metadata columns
    rel = rel.select(f"*, {seed} AS seed, '{content_version}' AS content_version, {metrics_every} AS metrics_every")
    return rel


def main() -> None:
    """CLI entry point: load a run directory and print summary stats."""
    if len(sys.argv) < 2:
        print("Usage: python3 scripts/analysis/load_run.py <run_dir>", file=sys.stderr)
        sys.exit(1)

    run_dir = sys.argv[1]
    rel = load_run(run_dir)

    row_count = rel.aggregate("count(*) AS total_rows").fetchone()
    seeds = rel.aggregate("count(DISTINCT seed) AS seed_count").fetchone()
    tick_range = rel.aggregate("min(tick) AS min_tick, max(tick) AS max_tick").fetchone()

    print(f"Loaded run: {run_dir}")
    assert row_count is not None
    assert seeds is not None
    assert tick_range is not None
    print(f"  Rows: {row_count[0]}")
    print(f"  Seeds: {seeds[0]}")
    print(f"  Tick range: {tick_range[0]} – {tick_range[1]}")
    print(f"\nSchema ({len(rel.columns)} columns):")
    for col in rel.columns:
        print(f"  {col}")


if __name__ == "__main__":
    main()
