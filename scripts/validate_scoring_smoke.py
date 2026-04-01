#!/usr/bin/env python3
"""Validate scoring_smoke summary.json for non-degenerate scores.

Usage: python3 scripts/validate_scoring_smoke.py <summary.json>

Exits 0 on success, 1 on any gate failure.
"""

import json
import sys

# Score metric names in summary.json (matches compute_summary output).
SCORE_DIMENSIONS = [
    "score_composite",
    "score_industrial_output",
    "score_research_progress",
    "score_economic_health",
    "score_fleet_operations",
    "score_efficiency",
    "score_expansion",
]


def main() -> int:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <summary.json>", file=sys.stderr)
        return 1

    with open(sys.argv[1]) as f:
        summary = json.load(f)

    # summary.json has: seed_count, collapsed_count, metrics (array of {name, mean, min, max, stddev})
    metrics_list = summary.get("metrics", [])
    metrics = {m["name"]: m for m in metrics_list}
    failures: list[str] = []

    # Gate 1: Composite score > 0
    composite = metrics.get("score_composite", {})
    composite_mean = composite.get("mean", 0)
    if composite_mean <= 0:
        failures.append(f"score_composite mean={composite_mean:.2f}, expected > 0")

    # Gate 2: All 6 sub-dimensions have non-zero mean
    for dim in SCORE_DIMENSIONS[1:]:  # skip composite
        dim_metrics = metrics.get(dim, {})
        dim_mean = dim_metrics.get("mean", 0)
        if dim_mean <= 0:
            failures.append(f"{dim} mean={dim_mean:.4f}, expected > 0")

    # Gate 3: No collapse
    collapsed = summary.get("collapsed_count", 0)
    if collapsed > 0:
        failures.append(f"collapsed_count={collapsed}, expected 0")

    if failures:
        print("scoring_smoke FAILED:", file=sys.stderr)
        for msg in failures:
            print(f"  - {msg}", file=sys.stderr)
        return 1

    print("scoring_smoke: all gates passed")
    print(
        f"  composite:    mean={composite_mean:>8.2f}"
        f"  min={composite.get('min', 0):.2f}"
        f"  max={composite.get('max', 0):.2f}"
    )
    for dim in SCORE_DIMENSIONS[1:]:
        dim_m = metrics.get(dim, {})
        print(f"  {dim:30s}: mean={dim_m.get('mean', 0):>8.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
