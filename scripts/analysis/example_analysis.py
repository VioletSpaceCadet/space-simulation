"""End-to-end demo: load a sim_bench run, compute features, print summary.

Usage:
    python3 scripts/analysis/example_analysis.py <run_dir>
"""

from __future__ import annotations

import sys
from pathlib import Path

from scripts.analysis.features import add_all_features
from scripts.analysis.load_run import load_run


def main() -> None:
    """Run the example analysis pipeline."""
    if len(sys.argv) < 2:
        print("Usage: python3 scripts/analysis/example_analysis.py <run_dir>", file=sys.stderr)
        sys.exit(1)

    run_dir = Path(sys.argv[1])
    print(f"Loading run: {run_dir}")
    rel = load_run(run_dir)

    print(f"Raw metrics: {rel.count('*').fetchone()[0]} rows")  # type: ignore[index]
    print(f"Columns: {len(rel.columns)}")

    # Add features
    featured = add_all_features(rel)
    print(f"\nWith features: {len(featured.columns)} columns")
    print(f"New feature columns: {len(featured.columns) - len(rel.columns)}")

    # Print summary statistics for key metrics
    summary = featured.aggregate(
        """
        count(*) AS total_rows,
        count(DISTINCT seed) AS seeds,
        max(tick) AS max_tick,
        avg(fleet_idle_ratio) AS avg_fleet_idle_ratio,
        avg(refinery_efficiency) AS avg_refinery_efficiency,
        avg(power_surplus_kw) AS avg_power_surplus_kw,
        sum(storage_critical) AS storage_critical_ticks,
        avg(material_kg_delta) AS avg_material_throughput
        """
    ).fetchone()

    assert summary is not None
    print("\n--- Summary ---")
    print(f"Total rows:             {summary[0]}")
    print(f"Seeds:                  {summary[1]}")
    print(f"Max tick:               {summary[2]}")
    print(f"Avg fleet idle ratio:   {summary[3]:.3f}")
    print(f"Avg refinery efficiency:{summary[4]:.3f}")
    print(f"Avg power surplus (kW): {summary[5]:.1f}")
    print(f"Storage critical ticks: {summary[6]}")
    print(f"Avg material throughput:{summary[7]:.2f} kg/sample")


if __name__ == "__main__":
    main()
