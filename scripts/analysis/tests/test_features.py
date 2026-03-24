"""Tests for features module."""

from __future__ import annotations

import duckdb

from scripts.analysis.features import (
    add_all_features,
    add_fleet_utilization,
    add_power_surplus,
    add_storage_pressure,
    add_throughput_rates,
)


def _make_test_relation() -> duckdb.DuckDBPyRelation:
    """Create a minimal DuckDB relation with raw metrics columns."""
    conn = duckdb.connect(":memory:")
    return conn.sql(
        """
        SELECT * FROM (VALUES
            (0, 10, 100.0, 50.0, 10.0, 0.5, 3, 1, 1, 2, 0, 1, 100.0, 75.0),
            (0, 20, 110.0, 60.0, 12.0, 0.8, 3, 0, 2, 2, 0, 1, 100.0, 75.0),
            (0, 30, 105.0, 70.0, 14.0, 0.96, 3, 2, 0, 2, 1, 0, 100.0, 75.0),
            (1, 10, 200.0, 80.0, 20.0, 0.3, 4, 1, 2, 3, 0, 0, 120.0, 90.0),
            (1, 20, 220.0, 90.0, 25.0, 0.4, 4, 0, 3, 3, 0, 0, 120.0, 90.0)
        ) AS t(
            seed, tick, total_ore_kg, total_material_kg, total_slag_kg,
            station_storage_used_pct, fleet_total, fleet_idle, fleet_mining,
            processor_active, processor_starved, processor_stalled,
            power_generated_kw, power_consumed_kw
        )
        """
    )


def test_throughput_rates_has_delta_columns() -> None:
    """Throughput rates adds material/ore/slag delta columns."""
    rel = _make_test_relation()
    result = add_throughput_rates(rel)
    assert "material_kg_delta" in result.columns
    assert "ore_kg_delta" in result.columns
    assert "slag_kg_delta" in result.columns


def test_throughput_rates_values() -> None:
    """Delta is computed per-seed using LAG window function."""
    rel = _make_test_relation()
    result = add_throughput_rates(rel)
    rows = result.select("seed, tick, material_kg_delta").order("seed, tick").fetchall()

    # seed=0: tick 10 → delta=0 (first row), tick 20 → 60-50=10, tick 30 → 70-60=10
    assert float(rows[0][2]) == 0.0  # first row, no previous
    assert abs(float(rows[1][2]) - 10.0) < 0.01
    assert abs(float(rows[2][2]) - 10.0) < 0.01

    # seed=1: tick 10 → 0, tick 20 → 90-80=10
    assert float(rows[3][2]) == 0.0
    assert abs(float(rows[4][2]) - 10.0) < 0.01


def test_fleet_utilization() -> None:
    """Fleet utilization ratios are computed correctly."""
    rel = _make_test_relation()
    result = add_fleet_utilization(rel)
    assert "fleet_idle_ratio" in result.columns
    assert "fleet_mining_ratio" in result.columns

    rows = result.select("seed, tick, fleet_idle_ratio, fleet_mining_ratio").order("seed, tick").fetchall()
    # seed=0, tick=10: idle=1/3, mining=1/3
    assert abs(float(rows[0][2]) - 1.0 / 3.0) < 0.01
    assert abs(float(rows[0][3]) - 1.0 / 3.0) < 0.01
    # seed=0, tick=20: idle=0/3, mining=2/3
    assert abs(float(rows[1][2]) - 0.0) < 0.01
    assert abs(float(rows[1][3]) - 2.0 / 3.0) < 0.01


def test_power_surplus() -> None:
    """Power surplus is generated - consumed."""
    rel = _make_test_relation()
    result = add_power_surplus(rel)
    rows = result.select("seed, tick, power_surplus_kw, power_surplus_ratio").order("seed, tick").fetchall()
    # First row: 100 - 75 = 25
    assert abs(float(rows[0][2]) - 25.0) < 0.01
    assert abs(float(rows[0][3]) - 0.25) < 0.01
    # seed=1: 120 - 90 = 30
    assert abs(float(rows[3][2]) - 30.0) < 0.01


def test_storage_pressure() -> None:
    """Storage critical flag and refinery efficiency."""
    rel = _make_test_relation()
    result = add_storage_pressure(rel)
    rows = result.select("seed, tick, storage_critical, refinery_efficiency").order("seed, tick").fetchall()
    # tick=10: storage=0.5 → not critical, refinery=2/(2+0+1)=0.667
    assert rows[0][2] == 0
    assert abs(float(rows[0][3]) - 2.0 / 3.0) < 0.01
    # tick=30: storage=0.96 → critical
    assert rows[2][2] == 1


def test_add_all_features() -> None:
    """All features compose without error."""
    rel = _make_test_relation()
    result = add_all_features(rel)
    expected_new = {
        "material_kg_delta",
        "ore_kg_delta",
        "slag_kg_delta",
        "fleet_idle_ratio",
        "fleet_mining_ratio",
        "power_surplus_kw",
        "power_surplus_ratio",
        "storage_critical",
        "refinery_efficiency",
    }
    result_cols = set(result.columns)
    assert expected_new.issubset(result_cols)
