#!/usr/bin/env python3
"""Validate progression_full_arc: autopilot progresses from ground through orbit.

Usage: python3 scripts/validate_progression_full_arc.py <run_dir>

Reads per-seed run_result.json files from the sim_bench run directory
and validates progression gates:
  - 80%+ seeds reach Orbital phase (game_phase >= 1)
  - 80%+ seeds complete at least 4 milestones
  - No collapses across any seed

Exits 0 on success, 1 on any gate failure.
"""
import json
import sys
from pathlib import Path

PHASE_NAMES = {0: "Startup", 1: "Orbital", 2: "Industrial", 3: "Expansion", 4: "DeepSpace"}


def main() -> int:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <run_dir>", file=sys.stderr)
        return 1

    run_dir = Path(sys.argv[1])
    seed_dirs = sorted(run_dir.glob("seed_*"))
    if not seed_dirs:
        print(f"ERROR: no seed_* directories found in {run_dir}", file=sys.stderr)
        return 1

    seed_count = len(seed_dirs)
    phases: list[int] = []
    milestones: list[int] = []
    collapses = 0

    for seed_dir in seed_dirs:
        result_path = seed_dir / "run_result.json"
        if not result_path.exists():
            print(f"ERROR: {result_path} not found", file=sys.stderr)
            return 1

        with open(result_path) as f:
            result = json.load(f)

        if result.get("collapse_occurred", False):
            collapses += 1

        metrics = result.get("summary_metrics", {})
        phases.append(metrics.get("game_phase", 0))
        milestones.append(metrics.get("milestones_completed", 0))

    failures: list[str] = []

    # Gate 1: No collapses
    if collapses > 0:
        failures.append(f"{collapses}/{seed_count} seeds collapsed")

    # Gate 2: 80%+ seeds reach Orbital (phase >= 1)
    orbital_count = sum(1 for p in phases if p >= 1)
    orbital_pct = orbital_count / seed_count * 100
    if orbital_pct < 80:
        failures.append(
            f"only {orbital_count}/{seed_count} ({orbital_pct:.0f}%) seeds "
            f"reached Orbital, need 80%+"
        )

    # Gate 3: 80%+ seeds complete at least 4 milestones
    milestone_4_count = sum(1 for m in milestones if m >= 4)
    milestone_pct = milestone_4_count / seed_count * 100
    if milestone_pct < 80:
        failures.append(
            f"only {milestone_4_count}/{seed_count} ({milestone_pct:.0f}%) seeds "
            f"completed 4+ milestones, need 80%+"
        )

    # Print per-seed breakdown
    print(f"\n  progression_full_arc ({seed_count} seeds, 30k ticks from ground_start):")
    print(f"  {'Seed':<8} {'Phase':<15} {'Milestones':<12}")
    print(f"  {'-'*35}")
    for i, seed_dir in enumerate(seed_dirs):
        phase_name = PHASE_NAMES.get(phases[i], f"Unknown({phases[i]})")
        print(f"  {seed_dir.name:<8} {phase_name:<15} {milestones[i]:<12}")

    print(f"\n  Orbital+: {orbital_count}/{seed_count} ({orbital_pct:.0f}%)")
    print(f"  4+ milestones: {milestone_4_count}/{seed_count} ({milestone_pct:.0f}%)")
    print(f"  Collapses: {collapses}/{seed_count}")

    if failures:
        print("\n  FAIL: progression_full_arc validation failed:", file=sys.stderr)
        for msg in failures:
            print(f"    - {msg}", file=sys.stderr)
        return 1

    print("\n  progression_full_arc: all gates passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
