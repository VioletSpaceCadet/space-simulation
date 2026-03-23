"""Compute derived features from raw sim_bench metrics.

All functions take a DuckDB relation (from load_run) and return a new
relation with additional computed columns.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import duckdb


def add_throughput_rates(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Add per-tick delta columns for material production metrics.

    New columns:
        material_kg_delta: change in total_material_kg per tick
        ore_kg_delta: change in total_ore_kg per tick
        slag_kg_delta: change in total_slag_kg per tick
    """
    return rel.select(
        """*,
        total_material_kg - LAG(total_material_kg, 1, total_material_kg)
            OVER (PARTITION BY seed ORDER BY tick) AS material_kg_delta,
        total_ore_kg - LAG(total_ore_kg, 1, total_ore_kg)
            OVER (PARTITION BY seed ORDER BY tick) AS ore_kg_delta,
        total_slag_kg - LAG(total_slag_kg, 1, total_slag_kg)
            OVER (PARTITION BY seed ORDER BY tick) AS slag_kg_delta
        """
    )


def add_fleet_utilization(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Add fleet utilization ratio.

    New columns:
        fleet_idle_ratio: fleet_idle / fleet_total (0.0 if no ships)
        fleet_mining_ratio: fleet_mining / fleet_total
    """
    return rel.select(
        """*,
        CASE WHEN fleet_total > 0
            THEN CAST(fleet_idle AS FLOAT) / fleet_total
            ELSE 0.0
        END AS fleet_idle_ratio,
        CASE WHEN fleet_total > 0
            THEN CAST(fleet_mining AS FLOAT) / fleet_total
            ELSE 0.0
        END AS fleet_mining_ratio
        """
    )


def add_power_surplus(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Add power surplus/deficit metric.

    New columns:
        power_surplus_kw: power_generated_kw - power_consumed_kw
        power_surplus_ratio: surplus / generated (0.0 if no generation)
    """
    return rel.select(
        """*,
        power_generated_kw - power_consumed_kw AS power_surplus_kw,
        CASE WHEN power_generated_kw > 0
            THEN (power_generated_kw - power_consumed_kw) / power_generated_kw
            ELSE 0.0
        END AS power_surplus_ratio
        """
    )


def add_storage_pressure(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Add storage pressure indicators.

    New columns:
        storage_critical: 1 if station_storage_used_pct > 0.95, else 0
        refinery_efficiency: active / (active + starved + stalled), 0 if none
    """
    return rel.select(
        """*,
        CASE WHEN station_storage_used_pct > 0.95 THEN 1 ELSE 0 END AS storage_critical,
        CASE WHEN (refinery_active_count + refinery_starved_count + refinery_stalled_count) > 0
            THEN CAST(refinery_active_count AS FLOAT)
                / (refinery_active_count + refinery_starved_count + refinery_stalled_count)
            ELSE 0.0
        END AS refinery_efficiency
        """
    )


def add_all_features(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Apply all feature computations in sequence.

    Convenience function that chains all feature additions.
    """
    rel = add_throughput_rates(rel)
    rel = add_fleet_utilization(rel)
    rel = add_power_surplus(rel)
    return add_storage_pressure(rel)
