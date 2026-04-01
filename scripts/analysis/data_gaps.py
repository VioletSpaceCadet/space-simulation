"""Data gap detection for scoring dimensions.

Analyzes sim_bench Parquet output to identify scoring dimensions with
missing or thin metric coverage. Flags zero-variance dimensions,
all-zero values, and temporally static signals.

Usage:
    python3 scripts/analysis/data_gaps.py <run_dir> [--json <output.json>]
"""

from __future__ import annotations

import json
import sys
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    import duckdb

# Score dimension columns in sim_bench Parquet output.
SCORE_DIMENSIONS = [
    "score_composite",
    "score_industrial",
    "score_research",
    "score_economic",
    "score_fleet",
    "score_efficiency",
    "score_expansion",
]


def dimension_stats(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Compute per-dimension statistics at the final tick across all seeds.

    Returns:
        Relation with columns: dimension, mean, stddev, min_val, max_val,
        seed_count, all_zero (bool), zero_variance (bool).
    """
    return rel.query(
        "metrics",
        """
        WITH final_ticks AS (
            SELECT seed, MAX(tick) AS max_tick
            FROM metrics
            GROUP BY seed
        ),
        final_scores AS (
            SELECT m.score_composite, m.score_industrial, m.score_research,
                m.score_economic, m.score_fleet, m.score_efficiency,
                m.score_expansion
            FROM metrics m
            JOIN final_ticks f ON m.seed = f.seed AND m.tick = f.max_tick
        ),
        unpivoted AS (
            SELECT 'score_composite' AS dimension, score_composite AS value FROM final_scores
            UNION ALL
            SELECT 'score_industrial', score_industrial FROM final_scores
            UNION ALL
            SELECT 'score_research', score_research FROM final_scores
            UNION ALL
            SELECT 'score_economic', score_economic FROM final_scores
            UNION ALL
            SELECT 'score_fleet', score_fleet FROM final_scores
            UNION ALL
            SELECT 'score_efficiency', score_efficiency FROM final_scores
            UNION ALL
            SELECT 'score_expansion', score_expansion FROM final_scores
        )
        SELECT dimension,
            AVG(value) AS mean,
            COALESCE(STDDEV_SAMP(value), 0.0) AS stddev,
            MIN(value) AS min_val,
            MAX(value) AS max_val,
            COUNT(*) AS seed_count,
            MAX(ABS(value)) = 0 AS all_zero,
            COALESCE(STDDEV_SAMP(value), 0.0) < 1e-10 AS zero_variance
        FROM unpivoted
        GROUP BY dimension
        ORDER BY dimension
        """,
    )


def temporal_change(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Detect dimensions that are temporally static (no change over time).

    Compares the first and last score for each seed. A dimension is
    "static" if the delta is zero for every seed.

    Returns:
        Relation with columns: dimension, avg_delta, max_abs_delta,
        static (bool — true if no seed shows any change).
    """
    return rel.query(
        "metrics",
        """
        WITH first_ticks AS (
            SELECT seed, MIN(tick) AS min_tick
            FROM metrics
            WHERE score_composite IS NOT NULL
            GROUP BY seed
        ),
        last_ticks AS (
            SELECT seed, MAX(tick) AS max_tick
            FROM metrics
            WHERE score_composite IS NOT NULL
            GROUP BY seed
        ),
        first_scores AS (
            SELECT m.seed, m.score_composite, m.score_industrial,
                m.score_research, m.score_economic, m.score_fleet,
                m.score_efficiency, m.score_expansion
            FROM metrics m
            JOIN first_ticks f ON m.seed = f.seed AND m.tick = f.min_tick
        ),
        last_scores AS (
            SELECT m.seed, m.score_composite, m.score_industrial,
                m.score_research, m.score_economic, m.score_fleet,
                m.score_efficiency, m.score_expansion
            FROM metrics m
            JOIN last_ticks l ON m.seed = l.seed AND m.tick = l.max_tick
        ),
        deltas AS (
            SELECT 'score_composite' AS dimension,
                AVG(l.score_composite - f.score_composite) AS avg_delta,
                MAX(ABS(l.score_composite - f.score_composite)) AS max_abs_delta
            FROM first_scores f JOIN last_scores l ON f.seed = l.seed
            UNION ALL
            SELECT 'score_industrial',
                AVG(l.score_industrial - f.score_industrial),
                MAX(ABS(l.score_industrial - f.score_industrial))
            FROM first_scores f JOIN last_scores l ON f.seed = l.seed
            UNION ALL
            SELECT 'score_research',
                AVG(l.score_research - f.score_research),
                MAX(ABS(l.score_research - f.score_research))
            FROM first_scores f JOIN last_scores l ON f.seed = l.seed
            UNION ALL
            SELECT 'score_economic',
                AVG(l.score_economic - f.score_economic),
                MAX(ABS(l.score_economic - f.score_economic))
            FROM first_scores f JOIN last_scores l ON f.seed = l.seed
            UNION ALL
            SELECT 'score_fleet',
                AVG(l.score_fleet - f.score_fleet),
                MAX(ABS(l.score_fleet - f.score_fleet))
            FROM first_scores f JOIN last_scores l ON f.seed = l.seed
            UNION ALL
            SELECT 'score_efficiency',
                AVG(l.score_efficiency - f.score_efficiency),
                MAX(ABS(l.score_efficiency - f.score_efficiency))
            FROM first_scores f JOIN last_scores l ON f.seed = l.seed
            UNION ALL
            SELECT 'score_expansion',
                AVG(l.score_expansion - f.score_expansion),
                MAX(ABS(l.score_expansion - f.score_expansion))
            FROM first_scores f JOIN last_scores l ON f.seed = l.seed
        )
        SELECT dimension, avg_delta, max_abs_delta,
            max_abs_delta < 1e-10 AS static
        FROM deltas
        ORDER BY dimension
        """,
    )


def build_gap_report(
    stats: duckdb.DuckDBPyRelation,
    temporal: duckdb.DuckDBPyRelation,
) -> dict[str, Any]:
    """Build a structured gap report from dimension stats and temporal analysis.

    Returns:
        Dict with keys: dimensions (list of per-dimension findings), gaps (list
        of flagged issues), summary (human-readable text).
    """
    stats_rows = stats.fetchall()
    temporal_rows = temporal.fetchall()

    temporal_map: dict[str, tuple[float, float, bool]] = {}
    for row in temporal_rows:
        temporal_map[str(row[0])] = (float(row[1]), float(row[2]), bool(row[3]))

    dimensions: list[dict[str, Any]] = []
    gaps: list[dict[str, str]] = []

    for row in stats_rows:
        name = str(row[0])
        mean = float(row[1])
        stddev = float(row[2])
        min_val = float(row[3])
        max_val = float(row[4])
        seed_count = int(row[5])
        all_zero = bool(row[6])
        zero_variance = bool(row[7])

        avg_delta, max_abs_delta, static = temporal_map.get(name, (0.0, 0.0, True))

        entry: dict[str, Any] = {
            "dimension": name,
            "mean": mean,
            "stddev": stddev,
            "min": min_val,
            "max": max_val,
            "seed_count": seed_count,
            "all_zero": all_zero,
            "zero_variance": zero_variance,
            "temporal_static": static,
            "avg_delta": avg_delta,
            "max_abs_delta": max_abs_delta,
        }
        dimensions.append(entry)

        if all_zero:
            gaps.append(
                {
                    "dimension": name,
                    "issue": "all_zero",
                    "detail": f"{name} is zero across all {seed_count} seeds",
                }
            )
        if zero_variance and not all_zero:
            gaps.append(
                {
                    "dimension": name,
                    "issue": "zero_variance",
                    "detail": f"{name} has no variance across {seed_count} seeds (mean={mean:.4f})",
                }
            )
        if static and not all_zero:
            gaps.append(
                {
                    "dimension": name,
                    "issue": "temporal_static",
                    "detail": f"{name} shows no change over time (max_delta={max_abs_delta:.6f})",
                }
            )

    summary_lines = [f"Gap analysis: {len(dimensions)} dimensions, {len(gaps)} gaps found"]
    for gap in gaps:
        summary_lines.append(f"  [{gap['issue']}] {gap['detail']}")
    if not gaps:
        summary_lines.append("  No gaps detected — all dimensions have signal and variance")

    return {
        "dimensions": dimensions,
        "gaps": gaps,
        "summary": "\n".join(summary_lines),
    }


def main() -> None:
    """CLI entry point: analyze a sim_bench run directory for data gaps."""
    if len(sys.argv) < 2:
        print(
            "Usage: python3 scripts/analysis/data_gaps.py <run_dir> [--json <output.json>]",
            file=sys.stderr,
        )
        sys.exit(1)

    from scripts.analysis.load_run import load_run

    run_dir = sys.argv[1]
    rel = load_run(run_dir)

    stats = dimension_stats(rel)
    temporal = temporal_change(rel)
    report = build_gap_report(stats, temporal)

    print(report["summary"])

    # Optional JSON export
    json_idx = None
    for i, arg in enumerate(sys.argv):
        if arg == "--json":
            json_idx = i
            break
    if json_idx is not None:
        json_path = sys.argv[json_idx + 1] if json_idx + 1 < len(sys.argv) else "gap_report.json"
        with open(json_path, "w") as f:
            json.dump(report, f, indent=2)
        print(f"\nJSON report written to: {json_path}")


if __name__ == "__main__":
    main()
