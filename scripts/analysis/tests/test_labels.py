"""Tests for outcome labeling module."""

from __future__ import annotations

import duckdb

from scripts.analysis.labels import (
    bottleneck_timeline,
    collapse_detection,
    final_score,
    research_stall_tick,
    storage_saturation_tick,
)

# --- collapse_detection ---


def test_collapse_balance_zero() -> None:
    """Detect collapse when balance hits zero."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick >= 300 THEN 0.0 ELSE 1e6 END AS balance,
            3 AS fleet_total, 1 AS fleet_idle
        FROM generate_series(1, 500) AS t(tick)
    """)
    result = collapse_detection(rel)
    rows = result.order("seed").fetchall()
    assert len(rows) == 1
    assert rows[0][1] == 300
    assert rows[0][2] == "balance_zero"


def test_collapse_fleet_idle() -> None:
    """Detect collapse when all ships idle for 500+ ticks."""
    conn = duckdb.connect(":memory:")
    # Normal 1-99, all idle 100-700 (span = 600 >= 499)
    rel = conn.sql("""
        SELECT 0 AS seed, tick, 1e6 AS balance,
            3 AS fleet_total,
            CASE WHEN tick >= 100 THEN 3 ELSE 1 END AS fleet_idle
        FROM generate_series(1, 700) AS t(tick)
    """)
    result = collapse_detection(rel)
    rows = result.fetchall()
    assert rows[0][1] == 100
    assert rows[0][2] == "fleet_idle"


def test_collapse_none() -> None:
    """No collapse when conditions not met."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT 0 AS seed, tick, 1e6 AS balance,
            3 AS fleet_total, 1 AS fleet_idle
        FROM generate_series(1, 200) AS t(tick)
    """)
    result = collapse_detection(rel)
    rows = result.fetchall()
    assert rows[0][1] is None
    assert rows[0][2] is None


def test_collapse_earliest_wins() -> None:
    """When both conditions met, earliest collapse tick wins."""
    conn = duckdb.connect(":memory:")
    # Fleet idle from 100 (span 600 ticks), balance zero from 200
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick >= 200 THEN 0.0 ELSE 1e6 END AS balance,
            3 AS fleet_total,
            CASE WHEN tick >= 100 THEN 3 ELSE 1 END AS fleet_idle
        FROM generate_series(1, 700) AS t(tick)
    """)
    result = collapse_detection(rel)
    rows = result.fetchall()
    assert rows[0][1] == 100
    assert rows[0][2] == "fleet_idle"


def test_collapse_fleet_idle_too_short() -> None:
    """Fleet idle run shorter than 500 ticks is not a collapse."""
    conn = duckdb.connect(":memory:")
    # All idle for only 300 ticks (300-599, span=299 < 499)
    rel = conn.sql("""
        SELECT 0 AS seed, tick, 1e6 AS balance,
            3 AS fleet_total,
            CASE WHEN tick BETWEEN 300 AND 599 THEN 3 ELSE 1 END AS fleet_idle
        FROM generate_series(1, 800) AS t(tick)
    """)
    result = collapse_detection(rel)
    rows = result.fetchall()
    assert rows[0][1] is None
    assert rows[0][2] is None


# --- storage_saturation_tick ---


def test_storage_saturation_sustained() -> None:
    """Detect sustained storage saturation (100+ ticks)."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick BETWEEN 200 AND 400 THEN 0.97 ELSE 0.5 END
                AS station_storage_used_pct
        FROM generate_series(1, 500) AS t(tick)
    """)
    result = storage_saturation_tick(rel)
    rows = result.fetchall()
    assert rows[0][1] == 200


def test_storage_saturation_brief() -> None:
    """No detection for brief saturation (<100 ticks)."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick BETWEEN 200 AND 250 THEN 0.97 ELSE 0.5 END
                AS station_storage_used_pct
        FROM generate_series(1, 500) AS t(tick)
    """)
    result = storage_saturation_tick(rel)
    rows = result.fetchall()
    assert rows[0][1] is None


def test_storage_saturation_multiple_seeds() -> None:
    """Each seed detected independently."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick BETWEEN 100 AND 300 THEN 0.97 ELSE 0.5 END
                AS station_storage_used_pct
        FROM generate_series(1, 500) AS t(tick)
        UNION ALL
        SELECT 1 AS seed, tick,
            0.5 AS station_storage_used_pct
        FROM generate_series(1, 500) AS t(tick)
    """)
    result = storage_saturation_tick(rel)
    rows = result.order("seed").fetchall()
    assert len(rows) == 2
    assert rows[0][1] == 100  # seed 0 saturated
    assert rows[1][1] is None  # seed 1 never saturated


# --- research_stall_tick ---


def test_research_stall_after_threshold() -> None:
    """Detect research stall after tick 1000."""
    conn = duckdb.connect(":memory:")
    # Increases until 1100, then flat to 1700 (600 ticks flat >= 499)
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick <= 1100
                THEN CAST(tick AS FLOAT)
                ELSE 1100.0
            END AS total_scan_data
        FROM generate_series(1, 1700) AS t(tick)
    """)
    result = research_stall_tick(rel)
    rows = result.fetchall()
    assert rows[0][1] == 1101


def test_research_stall_early_ignored() -> None:
    """Stalls before tick 1000 are not counted."""
    conn = duckdb.connect(":memory:")
    # Flat until 900, then increasing through 2000
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick <= 900 THEN 100.0
                ELSE 100.0 + CAST(tick - 900 AS FLOAT)
            END AS total_scan_data
        FROM generate_series(1, 2000) AS t(tick)
    """)
    result = research_stall_tick(rel)
    rows = result.fetchall()
    assert rows[0][1] is None


def test_research_stall_brief_not_detected() -> None:
    """Brief stall (<500 ticks) after tick 1000 not detected."""
    conn = duckdb.connect(":memory:")
    # Flat for only 200 ticks (1101-1300), then increasing again
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE
                WHEN tick <= 1100 THEN CAST(tick AS FLOAT)
                WHEN tick <= 1300 THEN 1100.0
                ELSE 1100.0 + CAST(tick - 1300 AS FLOAT)
            END AS total_scan_data
        FROM generate_series(1, 2000) AS t(tick)
    """)
    result = research_stall_tick(rel)
    rows = result.fetchall()
    assert rows[0][1] is None


# --- final_score ---


def test_final_score_uses_last_tick() -> None:
    """Score computed from the final tick per seed."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 1e9, 10, 5, 50.0),
            (0, 200, 1e9, 10, 5, 100.0)
        ) AS t(seed, tick, balance, techs_unlocked, fleet_total, total_material_kg)
    """)
    result = final_score(rel)
    rows = result.fetchall()
    assert len(rows) == 1
    assert rows[0][2] == 200  # final_tick
    score = float(rows[0][1])
    assert 0.0 < score <= 1.5


def test_final_score_zero_everything() -> None:
    """Score is zero when all inputs are zero."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 0.0, 0, 0, 0.0)
        ) AS t(seed, tick, balance, techs_unlocked, fleet_total, total_material_kg)
    """)
    result = final_score(rel)
    rows = result.fetchall()
    assert float(rows[0][1]) == 0.0


def test_final_score_multiple_seeds() -> None:
    """Higher-performing seed gets higher score."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 100, 1e9, 10, 5, 50.0),
            (1, 100, 0.0, 0, 0, 0.0)
        ) AS t(seed, tick, balance, techs_unlocked, fleet_total, total_material_kg)
    """)
    result = final_score(rel)
    rows = result.order("seed").fetchall()
    assert len(rows) == 2
    assert float(rows[0][1]) > float(rows[1][1])


def test_final_score_components() -> None:
    """Verify individual score components contribute correctly."""
    conn = duckdb.connect(":memory:")
    # Only balance contributing (techs=0, fleet=0, material=0)
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1000, 1e9, 0, 0, 0.0)
        ) AS t(seed, tick, balance, techs_unlocked, fleet_total, total_material_kg)
    """)
    result = final_score(rel)
    rows = result.fetchall()
    score = float(rows[0][1])
    # Only balance component: 0.3 * ln(1e9+1)/ln(1e9+1) = 0.3
    assert abs(score - 0.3) < 0.01


# --- bottleneck_timeline ---


def test_bottleneck_classification() -> None:
    """Each bottleneck type classified correctly by priority."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 0.97, 0.3, 10.0, 100.0, 2, 0, 3, 1),
            (0, 2, 0.5, 0.85, 10.0, 100.0, 2, 0, 3, 1),
            (0, 3, 0.5, 0.3, 200.0, 100.0, 2, 0, 3, 1),
            (0, 4, 0.5, 0.3, 10.0, 100.0, 1, 3, 3, 1),
            (0, 5, 0.5, 0.3, 10.0, 100.0, 2, 0, 3, 2),
            (0, 6, 0.5, 0.3, 10.0, 100.0, 2, 0, 3, 1)
        ) AS t(seed, tick, station_storage_used_pct, max_module_wear,
               total_slag_kg, total_material_kg, processor_active,
               processor_starved, fleet_total, fleet_idle)
    """)
    result = bottleneck_timeline(rel)
    rows = result.order("tick_start").fetchall()
    types = [r[3] for r in rows]
    assert types == [
        "StorageFull",
        "WearCritical",
        "SlagBackpressure",
        "OreSupply",
        "FleetIdle",
        "Healthy",
    ]


def test_bottleneck_consecutive_merge() -> None:
    """Consecutive same-type ticks merge into a single span."""
    conn = duckdb.connect(":memory:")
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            0.5 AS station_storage_used_pct,
            0.3 AS max_module_wear,
            10.0 AS total_slag_kg,
            100.0 AS total_material_kg,
            2 AS processor_active,
            0 AS processor_starved,
            3 AS fleet_total,
            1 AS fleet_idle
        FROM generate_series(1, 50) AS t(tick)
    """)
    result = bottleneck_timeline(rel)
    rows = result.fetchall()
    assert len(rows) == 1
    assert rows[0][1] == 1
    assert rows[0][2] == 50
    assert rows[0][3] == "Healthy"


def test_bottleneck_spans_cover_range() -> None:
    """Spans cover the full tick range with type transitions."""
    conn = duckdb.connect(":memory:")
    # Healthy 1-50, StorageFull 51-100
    rel = conn.sql("""
        SELECT 0 AS seed, tick,
            CASE WHEN tick > 50 THEN 0.97 ELSE 0.5 END
                AS station_storage_used_pct,
            0.3 AS max_module_wear,
            10.0 AS total_slag_kg,
            100.0 AS total_material_kg,
            2 AS processor_active,
            0 AS processor_starved,
            3 AS fleet_total,
            1 AS fleet_idle
        FROM generate_series(1, 100) AS t(tick)
    """)
    result = bottleneck_timeline(rel)
    rows = result.order("tick_start").fetchall()
    assert len(rows) == 2
    # Healthy span
    assert rows[0][1] == 1
    assert rows[0][2] == 50
    assert rows[0][3] == "Healthy"
    # StorageFull span
    assert rows[1][1] == 51
    assert rows[1][2] == 100
    assert rows[1][3] == "StorageFull"


def test_bottleneck_priority_order() -> None:
    """Higher-priority conditions take precedence."""
    conn = duckdb.connect(":memory:")
    # Both StorageFull AND WearCritical conditions met — StorageFull wins
    rel = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 0.97, 0.9, 10.0, 100.0, 2, 0, 3, 1)
        ) AS t(seed, tick, station_storage_used_pct, max_module_wear,
               total_slag_kg, total_material_kg, processor_active,
               processor_starved, fleet_total, fleet_idle)
    """)
    result = bottleneck_timeline(rel)
    rows = result.fetchall()
    assert rows[0][3] == "StorageFull"
