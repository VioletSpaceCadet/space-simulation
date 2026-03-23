"""End-to-end ML pipeline smoke test.

Exercises the full pipeline: sim_bench -> Parquet -> DuckDB -> features
-> labels -> model stubs. Verifies non-empty output at each stage.

Usage:
    python3 scripts/analysis/smoke_test.py
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


def _check(condition: bool, message: str) -> None:
    """Assert with clear failure message."""
    if not condition:
        print(f"FAIL: {message}", file=sys.stderr)
        sys.exit(1)
    print(f"  OK: {message}")


def main() -> None:  # pragma: no cover
    """Run the full ML pipeline smoke test."""
    print("=== ML Pipeline Smoke Test ===\n")

    # Use a temp directory for output
    with tempfile.TemporaryDirectory() as tmpdir:
        output_dir = Path(tmpdir) / "ml_smoke"

        # Stage 1: Run sim_bench
        print("Stage 1: sim_bench (500 ticks, 1 seed)")
        scenario = Path("scenarios/ml_smoke.json")
        _check(scenario.exists(), f"scenario file exists: {scenario}")
        subprocess.run(
            [
                "cargo",
                "run",
                "--release",
                "-p",
                "sim_bench",
                "--",
                "run",
                "--scenario",
                str(scenario),
                str(output_dir),
            ],
            check=True,
        )
        seed_dirs = list(output_dir.glob("seed_*"))
        _check(len(seed_dirs) == 1, f"1 seed directory created ({len(seed_dirs)} found)")
        parquet_files = list(output_dir.glob("seed_*/metrics.parquet"))
        _check(len(parquet_files) == 1, "metrics.parquet created")
        print()

        # Stage 2: Load into DuckDB
        print("Stage 2: load_run (Parquet -> DuckDB)")
        from scripts.analysis.load_run import load_run

        rel = load_run(output_dir)
        row_count = rel.count("*").fetchone()
        assert row_count is not None
        _check(row_count[0] > 0, f"loaded {row_count[0]} rows")
        _check("seed" in rel.columns, "seed column present")
        _check("tick" in rel.columns, "tick column present")
        print()

        # Stage 3: Compute features
        print("Stage 3: features")
        from scripts.analysis.features import add_all_features

        featured = add_all_features(rel)
        _check("material_kg_delta" in featured.columns, "throughput rates added")
        _check("fleet_idle_ratio" in featured.columns, "fleet utilization added")
        _check("power_surplus_kw" in featured.columns, "power surplus added")
        _check("storage_critical" in featured.columns, "storage pressure added")
        print()

        # Stage 4: Apply labels
        print("Stage 4: labels")
        from scripts.analysis.labels import (
            bottleneck_timeline,
            collapse_detection,
            final_score,
        )

        collapses = collapse_detection(featured)
        collapse_rows = collapses.fetchall()
        _check(len(collapse_rows) == 1, f"collapse detection: {len(collapse_rows)} seed(s)")

        scores = final_score(featured)
        score_rows = scores.fetchall()
        _check(len(score_rows) == 1, f"final score: {len(score_rows)} seed(s)")
        score_val = float(score_rows[0][1])
        _check(0.0 <= score_val <= 2.0, f"score in valid range: {score_val:.3f}")

        timeline = bottleneck_timeline(featured)
        timeline_rows = timeline.fetchall()
        _check(len(timeline_rows) > 0, f"bottleneck timeline: {len(timeline_rows)} span(s)")
        # Verify non-overlapping
        for i in range(1, len(timeline_rows)):
            _check(
                timeline_rows[i][1] > timeline_rows[i - 1][2],
                f"spans non-overlapping at index {i}",
            )
        print()

        # Stage 5: Model stubs
        print("Stage 5: model stubs")
        from scripts.analysis.models.bottleneck_stub import (
            majority_class_evaluate,
            majority_class_train,
        )
        from scripts.analysis.models.scoring_stub import (
            mean_predict_evaluate,
            mean_predict_train,
        )

        majority = majority_class_train(timeline)
        _check(len(majority) > 0, f"bottleneck majority class: {majority}")
        acc = majority_class_evaluate(timeline, majority)
        _check(0.0 <= acc <= 1.0, f"bottleneck accuracy: {acc:.3f}")

        mean_pred = mean_predict_train(scores)
        _check(mean_pred >= 0.0, f"scoring mean prediction: {mean_pred:.4f}")
        mse = mean_predict_evaluate(scores, mean_pred)
        _check(mse >= 0.0, f"scoring MSE: {mse:.6f}")
        print()

        # Stage 6: Cross-seed analysis
        print("Stage 6: cross-seed analysis")
        from scripts.analysis.cross_seed import cross_seed_stats, seed_summary

        summary = seed_summary(featured)
        summary_rows = summary.fetchall()
        _check(len(summary_rows) == 1, f"seed summary: {len(summary_rows)} seed(s)")

        stats = cross_seed_stats(summary)
        stats_rows = stats.fetchall()
        _check(len(stats_rows) == 7, f"cross-seed stats: {len(stats_rows)} metrics")
        print()

        print("=== All stages passed ===")
        print(f"Rows loaded: {row_count[0]}")
        print(f"Feature columns: {len(featured.columns)}")
        print(f"Score: {score_val:.3f}")
        print(f"Bottleneck spans: {len(timeline_rows)}")
        print(f"Majority class: {majority} (accuracy: {acc:.3f})")


if __name__ == "__main__":
    main()
