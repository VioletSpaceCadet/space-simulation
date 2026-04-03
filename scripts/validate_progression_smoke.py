#!/usr/bin/env python3
"""Validate progression_ci_smoke batch_summary.json — no collapses allowed.

Usage: python3 scripts/validate_progression_smoke.py <batch_summary.json>

Exits 0 on success, 1 on any gate failure.
"""
import json
import sys


def main() -> int:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <batch_summary.json>", file=sys.stderr)
        return 1

    with open(sys.argv[1]) as f:
        summary = json.load(f)

    collapsed = summary.get("collapsed_count", 0)
    failures: list[str] = []

    # Gate: No collapse (progression_start.json must not deadlock)
    if collapsed > 0:
        failures.append(f"collapsed_count={collapsed}, expected 0")

    if failures:
        print("FAIL: progression smoke validation failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    seed_count = summary.get("seed_count", "?")
    print(f"  progression smoke validated ({seed_count} seeds, 0 collapses)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
