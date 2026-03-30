#!/usr/bin/env python3
"""Validate dev_state_smoke batch_summary.json against progression gates.

Usage: python3 scripts/validate_dev_state_smoke.py <batch_summary.json>

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

    metrics = summary["aggregated_metrics"]
    collapsed = summary.get("collapsed_count", 0)
    failures: list[str] = []

    # Gate 1: No collapse
    if collapsed > 0:
        failures.append(f"collapsed_count={collapsed}, expected 0")

    # Gate 2: Balance positive at end (mean across seeds)
    balance_mean = metrics["balance"]["mean"]
    if balance_mean <= 0:
        failures.append(f"balance mean={balance_mean:.0f}, expected > 0")

    # Gate 3: Power deficit is zero (no power starvation)
    deficit_max = metrics["power_deficit_kw"]["max"]
    if deficit_max > 0.01:
        failures.append(f"power_deficit_kw max={deficit_max:.2f}, expected 0")

    # Gate 4: Ships not permanently idle (fleet_idle < fleet_total at end)
    fleet_total_mean = metrics["fleet_total"]["mean"]
    fleet_idle_mean = metrics["fleet_idle"]["mean"]
    if fleet_total_mean > 0 and fleet_idle_mean >= fleet_total_mean:
        failures.append(
            f"fleet_idle mean={fleet_idle_mean:.1f} >= fleet_total={fleet_total_mean:.1f}"
            " (all ships idle)"
        )

    # Gate 5: Some ore was mined (mining pipeline works)
    ore_min = metrics["total_ore_kg"]["min"]
    material_min = metrics["total_material_kg"]["min"]
    if ore_min < 1.0 and material_min < 1.0:
        failures.append(
            f"total_ore_kg min={ore_min:.1f}, total_material_kg min={material_min:.1f}"
            " — no mining/refining happened"
        )

    # Gate 6: Some tech unlocked (research pipeline works)
    techs_min = metrics["techs_unlocked"]["min"]
    if techs_min < 1:
        failures.append(f"techs_unlocked min={techs_min}, expected >= 1")

    if failures:
        print("dev_state_smoke FAILED:", file=sys.stderr)
        for msg in failures:
            print(f"  - {msg}", file=sys.stderr)
        return 1

    print("dev_state_smoke: all gates passed")
    print(f"  balance:        {balance_mean:>12,.0f}")
    print(f"  techs_unlocked: {metrics['techs_unlocked']['mean']:>12.0f}")
    print(f"  fleet_idle:     {fleet_idle_mean:>12.1f} / {fleet_total_mean:.0f}")
    print(f"  power_deficit:  {deficit_max:>12.2f} kW")
    return 0


if __name__ == "__main__":
    sys.exit(main())
