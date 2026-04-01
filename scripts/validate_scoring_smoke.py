#!/usr/bin/env python3
"""Validate scoring_smoke batch_summary.json for non-degenerate scores.

Usage: python3 scripts/validate_scoring_smoke.py <batch_summary.json>

Exits 0 on success, 1 on any gate failure.
"""

import json
import sys

# Score dimension columns in batch_summary aggregated_metrics.
SCORE_DIMENSIONS = [
    "score_composite",
    "score_industrial",
    "score_research",
    "score_economic",
    "score_fleet",
    "score_efficiency",
    "score_expansion",
]


def main() -> int:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <batch_summary.json>", file=sys.stderr)
        return 1

    with open(sys.argv[1]) as f:
        summary = json.load(f)

    metrics = summary["aggregated_metrics"]
    failures: list[str] = []

    # Gate 1: Composite score > 0 (scoring pipeline produces non-zero output)
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

    # Gate 3: Composite score has reasonable range (not all seeds identical)
    composite_min = composite.get("min", 0)
    composite_max = composite.get("max", 0)
    if composite_max > 0 and (composite_max - composite_min) < 0.01:
        failures.append(
            f"score_composite range too narrow: min={composite_min:.2f}"
            f" max={composite_max:.2f} (possible determinism issue)"
        )

    # Gate 4: No collapse
    collapsed = summary.get("collapsed_count", 0)
    if collapsed > 0:
        failures.append(f"collapsed_count={collapsed}, expected 0")

    if failures:
        print("scoring_smoke FAILED:", file=sys.stderr)
        for msg in failures:
            print(f"  - {msg}", file=sys.stderr)
        return 1

    print("scoring_smoke: all gates passed")
    print(f"  composite:    mean={composite_mean:>8.2f}  min={composite_min:.2f}  max={composite_max:.2f}")
    for dim in SCORE_DIMENSIONS[1:]:
        dim_m = metrics.get(dim, {})
        print(f"  {dim:20s}: mean={dim_m.get('mean', 0):>8.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
