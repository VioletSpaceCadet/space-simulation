"""Tests for cross-seed analysis module."""

from __future__ import annotations

import duckdb

from scripts.analysis.cross_seed import (
    cross_seed_stats,
    determinism_check,
    seed_summary,
)

# --- seed_summary ---


def test_seed_summary_picks_final_tick() -> None:
    """Summary returns metrics from the last tick per seed."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 1e6, 5, 3, 50.0, 100.0, 10.0, 0.2),
            (0, 200, 2e6, 8, 4, 100.0, 200.0, 20.0, 0.3),
            (1, 100, 5e5, 3, 2, 30.0, 80.0, 5.0, 0.1)
        ) AS t(seed, tick, balance, techs_unlocked, fleet_total,
               total_material_kg, total_ore_kg, total_slag_kg, avg_module_wear)
    """)
    result = seed_summary(rel)
    rows = result.order("seed").fetchall()
    assert len(rows) == 2
    # Seed 0: final tick is 200
    assert rows[0][0] == 0
    assert rows[0][1] == 200
    assert float(rows[0][2]) == 2e6
    assert rows[0][3] == 8
    # Seed 1: final tick is 100
    assert rows[1][0] == 1
    assert rows[1][1] == 100


def test_seed_summary_all_columns_present() -> None:
    """Summary includes all expected columns."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 1e6, 5, 3, 50.0, 100.0, 10.0, 0.2)
        ) AS t(seed, tick, balance, techs_unlocked, fleet_total,
               total_material_kg, total_ore_kg, total_slag_kg, avg_module_wear)
    """)
    result = seed_summary(rel)
    expected = {
        "seed",
        "final_tick",
        "balance",
        "techs_unlocked",
        "fleet_total",
        "total_material_kg",
        "total_ore_kg",
        "total_slag_kg",
        "avg_module_wear",
    }
    assert set(result.columns) == expected


# --- cross_seed_stats ---


def test_cross_seed_stats_basic() -> None:
    """Stats compute mean/stddev/min/max for all metrics."""
    conn = duckdb.connect(":memory:")
    summary = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1000, 1e6, 10, 5, 100.0, 200.0, 20.0, 0.3),
            (1, 1000, 2e6, 8, 4, 80.0, 150.0, 15.0, 0.2),
            (2, 1000, 3e6, 12, 6, 120.0, 250.0, 25.0, 0.4)
        ) AS t(seed, final_tick, balance, techs_unlocked, fleet_total,
               total_material_kg, total_ore_kg, total_slag_kg, avg_module_wear)
    """)
    result = cross_seed_stats(summary)
    rows = result.fetchall()
    assert len(rows) == 7  # 7 metrics
    for row in rows:
        assert row[1] is not None  # mean
        assert float(row[2]) >= 0  # stddev
        assert row[3] is not None  # min_val
        assert row[4] is not None  # max_val
        assert float(row[5]) >= 0  # cv


def test_cross_seed_stats_single_seed() -> None:
    """Single seed produces zero stddev and cv."""
    conn = duckdb.connect(":memory:")
    summary = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1000, 1e6, 10, 5, 100.0, 200.0, 20.0, 0.3)
        ) AS t(seed, final_tick, balance, techs_unlocked, fleet_total,
               total_material_kg, total_ore_kg, total_slag_kg, avg_module_wear)
    """)
    result = cross_seed_stats(summary)
    rows = result.fetchall()
    for row in rows:
        assert float(row[2]) == 0.0  # stddev
        assert float(row[5]) == 0.0  # cv


def test_cross_seed_stats_cv_sorting() -> None:
    """Metrics sorted by coefficient of variation descending."""
    conn = duckdb.connect(":memory:")
    # Balance varies a lot, everything else constant
    summary = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1000, 1e6, 10, 5, 100.0, 200.0, 20.0, 0.3),
            (1, 1000, 5e6, 10, 5, 100.0, 200.0, 20.0, 0.3),
            (2, 1000, 9e6, 10, 5, 100.0, 200.0, 20.0, 0.3)
        ) AS t(seed, final_tick, balance, techs_unlocked, fleet_total,
               total_material_kg, total_ore_kg, total_slag_kg, avg_module_wear)
    """)
    result = cross_seed_stats(summary)
    rows = result.fetchall()
    # Balance has highest CV (~0.8), constant metrics have cv=0
    assert rows[0][0] == "balance"
    assert float(rows[0][5]) > 0.5  # cv > 0.5


def test_cross_seed_stats_mean_accuracy() -> None:
    """Mean computed correctly for known values."""
    conn = duckdb.connect(":memory:")
    summary = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1000, 100.0, 10, 4, 50.0, 100.0, 10.0, 0.2),
            (1, 1000, 200.0, 20, 6, 150.0, 300.0, 30.0, 0.4)
        ) AS t(seed, final_tick, balance, techs_unlocked, fleet_total,
               total_material_kg, total_ore_kg, total_slag_kg, avg_module_wear)
    """)
    result = cross_seed_stats(summary)
    rows = {r[0]: r for r in result.fetchall()}
    assert abs(float(rows["balance"][1]) - 150.0) < 0.01
    assert abs(float(rows["techs_unlocked"][1]) - 15.0) < 0.01
    assert abs(float(rows["fleet_total"][1]) - 5.0) < 0.01


# --- determinism_check ---


def test_determinism_check_clean() -> None:
    """No violations when each (seed, tick) is unique."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 1e6, 100.0, 50.0),
            (0, 200, 2e6, 200.0, 100.0),
            (1, 100, 5e5, 80.0, 30.0)
        ) AS t(seed, tick, balance, total_ore_kg, total_material_kg)
    """)
    result = determinism_check(rel)
    rows = result.fetchall()
    assert len(rows) == 0


def test_determinism_check_violation_balance() -> None:
    """Detect differing balance for same (seed, tick)."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 1e6, 100.0, 50.0),
            (0, 100, 2e6, 100.0, 50.0),
            (1, 100, 5e5, 80.0, 30.0)
        ) AS t(seed, tick, balance, total_ore_kg, total_material_kg)
    """)
    result = determinism_check(rel)
    rows = result.fetchall()
    assert len(rows) == 1
    assert rows[0][0] == 0  # seed
    assert rows[0][1] == 100  # tick
    assert rows[0][2] == 2  # row_count
    assert rows[0][3] == 2  # distinct_balance


def test_determinism_check_identical_duplicates_ok() -> None:
    """Identical duplicates are not flagged as violations."""
    conn = duckdb.connect(":memory:")
    # Same (seed, tick) with identical values
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 1e6, 100.0, 50.0),
            (0, 100, 1e6, 100.0, 50.0)
        ) AS t(seed, tick, balance, total_ore_kg, total_material_kg)
    """)
    result = determinism_check(rel)
    rows = result.fetchall()
    assert len(rows) == 0
