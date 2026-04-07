#!/usr/bin/env python3
"""Compare strategy_default vs strategy_optimized sim_bench results.

Usage: python3 scripts/validate_strategy_comparison.py <default_run_dir> <optimized_run_dir>

Reads summary.json and per-seed run_result.json files from both run
directories and validates:
  - Optimized composite score >= default (no regression)
  - No scoring dimension regresses by more than 20%
  - Both runs have zero collapses

Exits 0 on success, 1 on any gate failure.
"""
import json
import sys
from pathlib import Path


def load_summary(run_dir: Path) -> dict:
    """Load summary.json from a run directory."""
    path = run_dir / "summary.json"
    if not path.exists():
        print(f"ERROR: {path} not found", file=sys.stderr)
        sys.exit(1)
    with open(path) as f:
        return json.load(f)


def load_seed_results(run_dir: Path) -> list[dict]:
    """Load all per-seed run_result.json files from a run directory."""
    seed_dirs = sorted(run_dir.glob("seed_*"))
    results = []
    for seed_dir in seed_dirs:
        result_path = seed_dir / "run_result.json"
        if not result_path.exists():
            print(f"ERROR: {result_path} not found", file=sys.stderr)
            sys.exit(1)
        with open(result_path) as f:
            results.append(json.load(f))
    return results


def extract_dimension_means(summary: dict) -> dict[str, float]:
    """Extract score dimension means from summary.json."""
    dimensions: dict[str, float] = {}
    for metric in summary.get("metrics", []):
        name = metric["name"]
        if name.startswith("score_"):
            dimensions[name] = metric["mean"]
    return dimensions


def main() -> int:
    if len(sys.argv) != 3:
        print(
            f"Usage: {sys.argv[0]} <default_run_dir> <optimized_run_dir>",
            file=sys.stderr,
        )
        return 1

    default_dir = Path(sys.argv[1])
    optimized_dir = Path(sys.argv[2])

    default_summary = load_summary(default_dir)
    optimized_summary = load_summary(optimized_dir)
    default_results = load_seed_results(default_dir)
    optimized_results = load_seed_results(optimized_dir)

    seed_count = len(default_results)
    failures: list[str] = []

    # Gate 1: No collapses in either run
    for label, results in [("default", default_results), ("optimized", optimized_results)]:
        collapses = sum(1 for r in results if r.get("collapse_occurred", False))
        if collapses > 0:
            failures.append(f"{label}: {collapses}/{len(results)} seeds collapsed")

    # Extract dimension scores from summary.json
    default_dims = extract_dimension_means(default_summary)
    optimized_dims = extract_dimension_means(optimized_summary)

    default_composite = default_dims.get("score_composite", 0.0)
    optimized_composite = optimized_dims.get("score_composite", 0.0)
    delta_pct = (
        (optimized_composite - default_composite) / default_composite * 100
        if default_composite > 0
        else 0
    )

    # Gate 2: Optimized composite >= default (no regression)
    if optimized_composite < default_composite:
        failures.append(
            f"optimized composite ({optimized_composite:.2f}) < "
            f"default ({default_composite:.2f}), delta {delta_pct:+.1f}%"
        )

    # Gate 3: No dimension regresses by more than 20%
    dimension_results: list[tuple[str, float, float, float]] = []
    for dim in sorted(default_dims.keys()):
        if dim == "score_composite":
            continue
        default_mean = default_dims.get(dim, 0.0)
        optimized_mean = optimized_dims.get(dim, 0.0)
        if default_mean > 0.001:
            dim_delta_pct = (optimized_mean - default_mean) / default_mean * 100
        else:
            dim_delta_pct = 0.0
        dimension_results.append((dim, default_mean, optimized_mean, dim_delta_pct))

        if dim_delta_pct < -20:
            failures.append(
                f"dimension {dim} regressed: "
                f"{default_mean:.4f} -> {optimized_mean:.4f} ({dim_delta_pct:+.1f}%)"
            )

    # Print comparison table
    print(f"\n  Strategy comparison ({seed_count} seeds, 50k ticks):")
    print(f"  {'Metric':<30} {'Default':>10} {'Optimized':>10} {'Delta':>8}")
    print(f"  {'-'*60}")
    print(
        f"  {'score_composite':<30} {default_composite:>10.2f} "
        f"{optimized_composite:>10.2f} {delta_pct:>+7.1f}%"
    )
    for dim, default_mean, optimized_mean, dim_delta in dimension_results:
        print(
            f"  {dim:<30} {default_mean:>10.4f} "
            f"{optimized_mean:>10.4f} {dim_delta:>+7.1f}%"
        )

    # Per-seed composite variance
    default_composites = [r.get("score_composite", 0.0) or 0.0 for r in default_results]
    optimized_composites = [r.get("score_composite", 0.0) or 0.0 for r in optimized_results]
    default_stddev = (sum((x - default_composite) ** 2 for x in default_composites) / len(default_composites)) ** 0.5
    optimized_stddev = (sum((x - optimized_composite) ** 2 for x in optimized_composites) / len(optimized_composites)) ** 0.5
    print(f"\n  Variance: default stddev={default_stddev:.2f}, optimized stddev={optimized_stddev:.2f}")
    print(f"  Collapses: default={sum(1 for r in default_results if r.get('collapse_occurred', False))}, "
          f"optimized={sum(1 for r in optimized_results if r.get('collapse_occurred', False))}")

    if failures:
        print("\n  FAIL: strategy comparison failed:", file=sys.stderr)
        for msg in failures:
            print(f"    - {msg}", file=sys.stderr)
        return 1

    print(f"\n  strategy_comparison: optimized >= default ({delta_pct:+.1f}%), no dimension regressions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
