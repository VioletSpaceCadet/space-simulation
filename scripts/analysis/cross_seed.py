"""Cross-seed analysis for sim_bench metrics.

Compare metrics across multiple seeds to measure outcome variance,
identify RNG-sensitive bottlenecks, and verify determinism.

Usage:
    python3 scripts/analysis/cross_seed.py <run_dir> [--csv <output.csv>]
"""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import duckdb


def seed_summary(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Compute per-seed summary at the final tick.

    Returns relation with: seed, final_tick, balance, techs_unlocked,
    fleet_total, total_material_kg, total_ore_kg, total_slag_kg,
    avg_module_wear.
    """
    return rel.query(
        "metrics",
        """
        WITH final_ticks AS (
            SELECT seed, MAX(tick) AS max_tick
            FROM metrics
            GROUP BY seed
        )
        SELECT m.seed,
            m.tick AS final_tick,
            m.balance,
            m.techs_unlocked,
            m.fleet_total,
            m.total_material_kg,
            m.total_ore_kg,
            m.total_slag_kg,
            m.avg_module_wear
        FROM metrics m
        JOIN final_ticks f ON m.seed = f.seed AND m.tick = f.max_tick
        ORDER BY m.seed
        """,
    )


def cross_seed_stats(summary: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Compute aggregate statistics across seeds.

    Takes seed_summary() output. Returns per-metric statistics:
    metric name, mean, stddev, min, max, cv (coefficient of variation).
    Sorted by CV descending -- high-variance metrics first.
    """
    return summary.query(
        "s",
        """
        WITH unpivoted AS (
            SELECT 'balance' AS metric, CAST(balance AS DOUBLE) AS value FROM s
            UNION ALL
            SELECT 'techs_unlocked', CAST(techs_unlocked AS DOUBLE) FROM s
            UNION ALL
            SELECT 'fleet_total', CAST(fleet_total AS DOUBLE) FROM s
            UNION ALL
            SELECT 'total_material_kg', CAST(total_material_kg AS DOUBLE) FROM s
            UNION ALL
            SELECT 'total_ore_kg', CAST(total_ore_kg AS DOUBLE) FROM s
            UNION ALL
            SELECT 'total_slag_kg', CAST(total_slag_kg AS DOUBLE) FROM s
            UNION ALL
            SELECT 'avg_module_wear', CAST(avg_module_wear AS DOUBLE) FROM s
        )
        SELECT metric,
            AVG(value) AS mean,
            COALESCE(STDDEV_SAMP(value), 0.0) AS stddev,
            MIN(value) AS min_val,
            MAX(value) AS max_val,
            CASE WHEN AVG(value) != 0
                THEN COALESCE(STDDEV_SAMP(value), 0.0) / ABS(AVG(value))
                ELSE 0.0
            END AS cv
        FROM unpivoted
        GROUP BY metric
        ORDER BY cv DESC
        """,
    )


def determinism_check(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Check for determinism violations across duplicate (seed, tick) pairs.

    If the same (seed, tick) appears multiple times (e.g., from loading two
    runs of the same scenario), verifies metric values are identical.

    Returns relation with: seed, tick, row_count, distinct_balance,
    distinct_ore, distinct_material. Empty if fully deterministic.
    """
    return rel.query(
        "metrics",
        """
        SELECT seed, tick, COUNT(*) AS row_count,
            COUNT(DISTINCT balance) AS distinct_balance,
            COUNT(DISTINCT total_ore_kg) AS distinct_ore,
            COUNT(DISTINCT total_material_kg) AS distinct_material
        FROM metrics
        GROUP BY seed, tick
        HAVING COUNT(*) > 1
            AND (COUNT(DISTINCT balance) > 1
                OR COUNT(DISTINCT total_ore_kg) > 1
                OR COUNT(DISTINCT total_material_kg) > 1)
        ORDER BY seed, tick
        """,
    )


def main() -> None:
    """CLI entry point: load a run and print cross-seed analysis."""
    if len(sys.argv) < 2:
        print(
            "Usage: python3 scripts/analysis/cross_seed.py <run_dir> [--csv <output.csv>]",
            file=sys.stderr,
        )
        sys.exit(1)

    from scripts.analysis.load_run import load_run

    run_dir = sys.argv[1]
    rel = load_run(run_dir)

    summary = seed_summary(rel)
    stats = cross_seed_stats(summary)

    seed_count = summary.aggregate("count(*) AS n").fetchone()
    assert seed_count is not None
    print(f"=== Cross-Seed Analysis ({seed_count[0]} seeds) ===")
    print("\n--- Per-Seed Summary ---")
    summary.show()
    print("\n--- Aggregate Statistics (sorted by variance) ---")
    stats.show()

    violations = determinism_check(rel)
    violation_rows = violations.fetchall()
    if violation_rows:
        print(f"\nWARNING: {len(violation_rows)} determinism violations!")
        violations.show()
    else:
        print("\nDeterminism: OK")

    # Optional CSV export
    csv_idx = None
    for i, arg in enumerate(sys.argv):
        if arg == "--csv":
            csv_idx = i
            break
    if csv_idx is not None:
        csv_path = sys.argv[csv_idx + 1] if csv_idx + 1 < len(sys.argv) else "cross_seed_stats.csv"
        stats.write_csv(csv_path)
        print(f"\nExported to: {csv_path}")


if __name__ == "__main__":
    main()
