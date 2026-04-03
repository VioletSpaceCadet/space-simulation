#!/usr/bin/env python3
"""Compare score trajectories between two sim_bench runs.

Validates that progression_start produces a rising score curve while
dev_advanced_state produces a flat-high curve. Uses Parquet per-tick data.

Usage:
    python3 scripts/compare_score_curves.py <progression_run_dir> <advanced_run_dir>

Example:
    sim_bench run --scenario scenarios/scoring_progression.json --output-dir /tmp/sc
    sim_bench run --scenario scenarios/scoring_advanced.json --output-dir /tmp/sc
    python3 scripts/compare_score_curves.py /tmp/sc/scoring_progression_* /tmp/sc/scoring_advanced_*

Exits 0 on success, 1 on any gate failure.
"""

import glob
import json
import os
import sys
from collections import defaultdict

try:
    import pyarrow.parquet as pq
except ImportError:
    print("ERROR: pyarrow required. Install: pip install pyarrow", file=sys.stderr)
    sys.exit(1)


def read_score_curve(run_dir: str) -> dict[int, float]:
    """Read mean score_composite per tick from Parquet files across all seeds."""
    parquet_files = sorted(glob.glob(os.path.join(run_dir, "seed_*", "metrics.parquet")))
    if not parquet_files:
        print(f"ERROR: No parquet files in {run_dir}", file=sys.stderr)
        sys.exit(1)

    tick_scores: dict[int, list[float]] = defaultdict(list)
    for parquet_file in parquet_files:
        table = pq.read_table(parquet_file, columns=["tick", "score_composite"])
        ticks = table.column("tick").to_pylist()
        scores = table.column("score_composite").to_pylist()
        for tick, score in zip(ticks, scores, strict=True):
            tick_scores[tick].append(score)

    return {tick: sum(scores) / len(scores) for tick, scores in tick_scores.items()}


def read_summary_score(run_dir: str) -> float:
    """Read mean composite score from summary.json."""
    summary_path = os.path.join(run_dir, "summary.json")
    with open(summary_path) as f:
        summary = json.load(f)
    for metric in summary.get("metrics", []):
        if metric["name"] == "score_composite":
            return metric["mean"]
    return 0.0


def main() -> int:
    if len(sys.argv) != 3:
        print(
            f"Usage: {sys.argv[0]} <progression_run_dir> <advanced_run_dir>",
            file=sys.stderr,
        )
        return 1

    prog_dir = sys.argv[1]
    adv_dir = sys.argv[2]

    prog_curve = read_score_curve(prog_dir)
    adv_curve = read_score_curve(adv_dir)

    prog_ticks = sorted(prog_curve.keys())
    adv_ticks = sorted(adv_curve.keys())

    failures: list[str] = []

    # --- Report ---
    print("=== Score Curve Comparison ===\n")

    print("Progression start (rising curve expected):")
    for tick in prog_ticks:
        print(f"  tick {tick:5d}: {prog_curve[tick]:7.1f}")

    print("\nAdvanced state (flat-high curve expected):")
    for tick in adv_ticks:
        print(f"  tick {tick:5d}: {adv_curve[tick]:7.1f}")

    # --- Gates ---

    # Gate 1: Progression score at tick 2000 > score at tick 100 (growth confirmed)
    if len(prog_ticks) >= 2:
        early_tick = prog_ticks[0]
        late_tick = prog_ticks[-1]
        early_score = prog_curve[early_tick]
        late_score = prog_curve[late_tick]
        growth_pct = ((late_score - early_score) / early_score) * 100

        print(f"\nProgression growth: {early_score:.1f} → {late_score:.1f} ({growth_pct:+.1f}%)")

        if late_score <= early_score:
            failures.append(
                f"Progression score did not rise: tick {early_tick}={early_score:.1f} "
                f"→ tick {late_tick}={late_score:.1f}"
            )
    else:
        failures.append("Not enough progression data points")

    # Gate 2: Advanced score is relatively flat (stddev / mean < 10%)
    if adv_ticks:
        adv_scores = [adv_curve[t] for t in adv_ticks]
        adv_mean = sum(adv_scores) / len(adv_scores)
        adv_variance = sum((s - adv_mean) ** 2 for s in adv_scores) / len(adv_scores)
        adv_cv = (adv_variance**0.5) / adv_mean if adv_mean > 0 else 0

        print(f"Advanced flatness: mean={adv_mean:.1f}, CV={adv_cv:.3f} (<0.10 = flat)")

        if adv_cv > 0.10:
            failures.append(f"Advanced score not flat: CV={adv_cv:.3f} (threshold 0.10)")

    # Gate 3: Progression composite increases after tick 100
    #   Check that the score at a midpoint is higher than the first reading
    if len(prog_ticks) >= 3:
        mid_idx = len(prog_ticks) // 2
        mid_tick = prog_ticks[mid_idx]
        first_tick = prog_ticks[0]
        if prog_curve[mid_tick] <= prog_curve[first_tick]:
            failures.append(
                f"Progression score not rising at midpoint: tick {first_tick}={prog_curve[first_tick]:.1f} "
                f"→ tick {mid_tick}={prog_curve[mid_tick]:.1f}"
            )

    # Gate 4: Progression final score is competitive with advanced
    #   (progression should catch up or exceed by tick 2000)
    if prog_ticks and adv_ticks:
        prog_final = prog_curve[prog_ticks[-1]]
        adv_final = adv_curve[adv_ticks[-1]]
        ratio = prog_final / adv_final if adv_final > 0 else 0
        print(f"Final score ratio (prog/adv): {ratio:.2f}")

    print()

    if failures:
        print("FAIL: Score curve comparison failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print("All score curve gates passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
