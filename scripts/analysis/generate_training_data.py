"""Generate labeled training datasets from sim_bench runs.

Runs sim_bench for each ML scenario, then applies feature extraction
and outcome labeling to produce analysis-ready Parquet data.

Usage:
    python3 scripts/analysis/generate_training_data.py [--skip-sim]

The --skip-sim flag skips sim_bench runs and only processes existing
output (useful when recomputing features/labels on existing data).
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

# ML scenario files and output directory
SCENARIO_DIR = Path("scenarios")
OUTPUT_DIR = Path("data/training/v1")
ML_SCENARIOS = ["ml_baseline", "ml_constrained", "ml_abundant"]


def run_sim_bench(scenario_path: Path, output_dir: Path) -> None:
    """Run sim_bench for a single scenario."""
    output_dir.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cargo",
        "run",
        "--release",
        "-p",
        "sim_bench",
        "--",
        "run",
        "--scenario",
        str(scenario_path),
        str(output_dir),
    ]
    print(f"  Running: {' '.join(cmd)}")
    subprocess.run(cmd, check=True)


def process_run(run_dir: Path) -> dict[str, object]:
    """Load a run, apply features and labels, return summary stats."""
    from scripts.analysis.cross_seed import seed_summary
    from scripts.analysis.features import add_all_features
    from scripts.analysis.labels import (
        bottleneck_timeline,
        collapse_detection,
        final_score,
    )
    from scripts.analysis.load_run import load_run

    rel = load_run(run_dir)
    rel = add_all_features(rel)

    collapses = collapse_detection(rel)
    scores = final_score(rel)
    timeline = bottleneck_timeline(rel)
    summary = seed_summary(rel)

    # Gather stats
    seed_count = summary.aggregate("count(*) AS n").fetchone()
    collapse_rows = collapses.filter("collapse_tick IS NOT NULL").fetchall()
    score_stats = scores.aggregate(
        "AVG(score) AS mean_score, MIN(score) AS min_score, MAX(score) AS max_score"
    ).fetchone()

    timeline_types = timeline.query(
        "t",
        """
        SELECT bottleneck_type, COUNT(*) AS span_count
        FROM t
        GROUP BY bottleneck_type
        ORDER BY span_count DESC
        """,
    ).fetchall()

    assert seed_count is not None
    assert score_stats is not None
    return {
        "seeds": seed_count[0],
        "collapses": len(collapse_rows),
        "mean_score": float(score_stats[0]),
        "min_score": float(score_stats[1]),
        "max_score": float(score_stats[2]),
        "bottleneck_types": {str(r[0]): int(r[1]) for r in timeline_types},
    }


def main() -> None:
    """Generate training data for all ML scenarios."""
    skip_sim = "--skip-sim" in sys.argv

    print("=== ML Training Data Generation ===\n")

    if not skip_sim:
        print("Phase 1: Running sim_bench scenarios\n")
        for name in ML_SCENARIOS:
            scenario_path = SCENARIO_DIR / f"{name}.json"
            if not scenario_path.exists():
                print(f"  WARNING: {scenario_path} not found, skipping")
                continue
            output_dir = OUTPUT_DIR / name
            print(f"Scenario: {name}")
            run_sim_bench(scenario_path, output_dir)
            print()
    else:
        print("Phase 1: Skipped (--skip-sim)\n")

    print("Phase 2: Processing runs (features + labels)\n")
    all_stats: dict[str, dict[str, object]] = {}
    for name in ML_SCENARIOS:
        run_dir = OUTPUT_DIR / name
        if not run_dir.exists():
            print(f"  {name}: no output directory, skipping")
            continue
        print(f"  Processing: {name}")
        stats = process_run(run_dir)
        all_stats[name] = stats
        print(f"    Seeds: {stats['seeds']}, Collapses: {stats['collapses']}")
        print(f"    Score: {stats['mean_score']:.3f} (min={stats['min_score']:.3f}, max={stats['max_score']:.3f})")
        print(f"    Bottlenecks: {stats['bottleneck_types']}")
        print()

    print("=== Summary ===")
    total_seeds = 0
    total_collapses = 0
    for s in all_stats.values():
        seeds = s["seeds"]
        collapses = s["collapses"]
        assert isinstance(seeds, int)
        assert isinstance(collapses, int)
        total_seeds += seeds
        total_collapses += collapses
    print(f"Total seeds: {total_seeds}")
    print(f"Total collapses: {total_collapses}")
    print(f"Scenarios processed: {len(all_stats)}")


if __name__ == "__main__":
    main()
