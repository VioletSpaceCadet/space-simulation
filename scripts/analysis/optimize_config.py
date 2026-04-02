"""Grid search over AutopilotConfig parameters via sim_bench compare.

Generates config variants from a parameter grid, runs each through
sim_bench compare against the baseline, and ranks by composite score.

Usage:
    python3 scripts/analysis/optimize_config.py \\
        --baseline content/autopilot.json \\
        --grid grid.json \\
        --scenario scenarios/scoring_smoke.json \\
        --sim-bench target/release/sim_bench \\
        --output-dir runs/grid_search

Grid JSON example:
    {
        "slag_jettison_pct": [0.6, 0.75, 0.9],
        "refinery_threshold_kg": [1000.0, 2000.0, 3000.0]
    }
"""

from __future__ import annotations

import itertools
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def generate_variants(
    baseline: dict[str, Any],
    grid: dict[str, list[Any]],
) -> list[tuple[dict[str, Any], dict[str, Any]]]:
    """Generate config variants from a parameter grid.

    Returns list of (config_dict, overrides_dict) tuples. Each config_dict
    is a copy of baseline with the grid values applied. The overrides_dict
    records which parameters were changed.
    """
    param_names = list(grid.keys())
    param_values = [grid[name] for name in param_names]

    variants = []
    for combination in itertools.product(*param_values):
        overrides = dict(zip(param_names, combination, strict=True))
        config = {**baseline, **overrides}
        variants.append((config, overrides))
    return variants


def run_comparison(
    sim_bench_path: str,
    scenario_path: str,
    baseline_path: str,
    variant_config: dict[str, Any],
    output_dir: str,
    variant_label: str,
) -> dict[str, Any] | None:
    """Run sim_bench compare for a single variant. Returns parsed report or None on failure."""
    variant_dir = Path(output_dir) / variant_label
    variant_dir.mkdir(parents=True, exist_ok=True)
    variant_path = variant_dir / "config.json"
    variant_path.write_text(json.dumps(variant_config, indent=2))

    try:
        result = subprocess.run(
            [
                sim_bench_path,
                "compare",
                "--scenario",
                scenario_path,
                "--config-a",
                baseline_path,
                "--config-b",
                str(variant_path),
                "--output-dir",
                str(variant_dir),
            ],
            capture_output=True,
            text=True,
            timeout=600,
        )
    except subprocess.TimeoutExpired:
        print(f"  TIMEOUT: {variant_label}", file=sys.stderr)
        return None

    if result.returncode != 0:
        print(f"  FAILED: {variant_label}", file=sys.stderr)
        print(f"    {result.stderr[:200]}", file=sys.stderr)
        return None

    # Find the comparison_report.json in the output directory
    report_files = list(variant_dir.glob("compare_*/comparison_report.json"))
    if not report_files:
        print(f"  No report found for {variant_label}", file=sys.stderr)
        return None

    with open(report_files[0]) as f:
        report: dict[str, Any] = json.load(f)
        return report


def rank_variants(
    results: list[tuple[str, dict[str, Any], dict[str, Any]]],
) -> list[dict[str, Any]]:
    """Rank variant results by composite score delta (higher = better).

    Args:
        results: list of (label, overrides, report) tuples.

    Returns:
        Ranked list of dicts with label, overrides, composite_delta, dimension_deltas.
    """
    ranked = []
    for label, overrides, report in results:
        composite_delta = report.get("composite_delta", {})
        dimension_deltas = {dim["dimension_id"]: dim["delta"]["mean"] for dim in report.get("dimension_deltas", [])}
        ranked.append(
            {
                "rank": 0,  # filled below
                "label": label,
                "overrides": overrides,
                "composite_delta_mean": composite_delta.get("mean", 0.0),
                "composite_delta_stddev": composite_delta.get("stddev", 0.0),
                "significant": report.get("composite_t_test", {}).get("significant_at_05", False),
                "dimension_deltas": dimension_deltas,
            }
        )

    ranked.sort(key=lambda r: r["composite_delta_mean"], reverse=True)
    for index, entry in enumerate(ranked):
        entry["rank"] = index + 1

    return ranked


def print_rankings(ranked: list[dict[str, Any]]) -> None:
    """Print human-readable ranking table."""
    print(f"\n=== Grid Search Results ({len(ranked)} variants) ===\n")
    print(f"{'Rank':<6} {'Label':<30} {'Delta':>10} {'StdDev':>10} {'Sig?':>6}")
    print("-" * 65)
    for entry in ranked:
        sig = "YES" if entry["significant"] else "no"
        print(
            f"{entry['rank']:<6} {entry['label']:<30} "
            f"{entry['composite_delta_mean']:>+10.2f} "
            f"{entry['composite_delta_stddev']:>10.2f} "
            f"{sig:>6}"
        )
    if ranked:
        best = ranked[0]
        print(f"\nBest: {best['label']} (delta={best['composite_delta_mean']:+.2f})")
        print(f"  Overrides: {json.dumps(best['overrides'])}")


def run_grid_search(
    baseline_path: str,
    grid_path: str,
    scenario_path: str,
    sim_bench_path: str,
    output_dir: str,
) -> list[dict[str, Any]]:
    """Run grid search end-to-end. Returns ranked results."""
    with open(baseline_path) as f:
        baseline = json.load(f)
    with open(grid_path) as f:
        grid = json.load(f)

    variants = generate_variants(baseline, grid)
    print(f"Grid search: {len(variants)} variants from {len(grid)} parameters")

    results: list[tuple[str, dict[str, Any], dict[str, Any]]] = []
    for index, (config, overrides) in enumerate(variants):
        label = "_".join(f"{k}={v}" for k, v in overrides.items())
        print(f"\n[{index + 1}/{len(variants)}] {label}")

        report = run_comparison(
            sim_bench_path,
            scenario_path,
            baseline_path,
            config,
            output_dir,
            label,
        )
        if report is not None:
            results.append((label, overrides, report))

    ranked = rank_variants(results)
    print_rankings(ranked)

    # Write ranked results
    results_path = Path(output_dir) / "grid_search_results.json"
    results_path.parent.mkdir(parents=True, exist_ok=True)
    with open(results_path, "w") as f:
        json.dump(ranked, f, indent=2)
    print(f"\nResults written to: {results_path}")

    return ranked


def main() -> None:
    """CLI entry point."""
    import argparse

    parser = argparse.ArgumentParser(description="Grid search over AutopilotConfig parameters")
    parser.add_argument("--baseline", required=True, help="Path to baseline autopilot config JSON")
    parser.add_argument("--grid", required=True, help="Path to parameter grid JSON")
    parser.add_argument("--scenario", required=True, help="Path to sim_bench scenario JSON")
    parser.add_argument(
        "--sim-bench",
        default="target/release/sim_bench",
        help="Path to sim_bench binary",
    )
    parser.add_argument("--output-dir", default="runs/grid_search", help="Output directory")
    args = parser.parse_args()

    run_grid_search(args.baseline, args.grid, args.scenario, args.sim_bench, args.output_dir)


if __name__ == "__main__":
    main()
