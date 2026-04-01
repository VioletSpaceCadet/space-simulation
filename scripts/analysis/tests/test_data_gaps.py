"""Tests for data gap detection module."""

from __future__ import annotations

import duckdb

from scripts.analysis.data_gaps import (
    build_gap_report,
    dimension_stats,
    temporal_change,
)


def _make_score_relation(
    conn: duckdb.DuckDBPyConnection,
    rows: list[tuple[int, int, float, float, float, float, float, float, float]],
) -> duckdb.DuckDBPyRelation:
    """Helper: create a relation with score columns from value tuples."""
    values = ", ".join(f"({r[0]}, {r[1]}, {r[2]}, {r[3]}, {r[4]}, {r[5]}, {r[6]}, {r[7]}, {r[8]})" for r in rows)
    return conn.sql(f"""
        SELECT * FROM (VALUES {values})
        AS t(seed, tick, score_composite, score_industrial, score_research,
             score_economic, score_fleet, score_efficiency, score_expansion)
    """)  # noqa: S608 — test helper with controlled numeric input


# --- dimension_stats ---


def test_dimension_stats_basic() -> None:
    """Compute stats across two seeds with different scores."""
    conn = duckdb.connect(":memory:")
    rel = _make_score_relation(
        conn,
        [
            (0, 100, 400.0, 0.6, 0.5, 0.4, 0.8, 0.7, 0.3),
            (1, 100, 600.0, 0.8, 0.7, 0.6, 0.9, 0.8, 0.5),
        ],
    )
    result = dimension_stats(rel)
    rows = result.fetchall()
    assert len(rows) == 7
    # All dimensions should have seed_count=2, not all_zero, not zero_variance
    for row in rows:
        assert int(row[5]) == 2  # seed_count
        assert not bool(row[6])  # all_zero
        assert not bool(row[7])  # zero_variance


def test_dimension_stats_all_zero() -> None:
    """Detect all-zero dimension."""
    conn = duckdb.connect(":memory:")
    # expansion is 0.0 for both seeds
    rel = _make_score_relation(
        conn,
        [
            (0, 100, 400.0, 0.6, 0.5, 0.4, 0.8, 0.7, 0.0),
            (1, 100, 600.0, 0.8, 0.7, 0.6, 0.9, 0.8, 0.0),
        ],
    )
    result = dimension_stats(rel)
    rows = result.fetchall()
    expansion = next(r for r in rows if r[0] == "score_expansion")
    assert bool(expansion[6])  # all_zero


def test_dimension_stats_zero_variance() -> None:
    """Detect zero-variance dimension (same non-zero value across seeds)."""
    conn = duckdb.connect(":memory:")
    # fleet is exactly 0.5 for both seeds
    rel = _make_score_relation(
        conn,
        [
            (0, 100, 400.0, 0.6, 0.5, 0.4, 0.5, 0.7, 0.3),
            (1, 100, 600.0, 0.8, 0.7, 0.6, 0.5, 0.8, 0.5),
        ],
    )
    result = dimension_stats(rel)
    rows = result.fetchall()
    fleet = next(r for r in rows if r[0] == "score_fleet")
    assert bool(fleet[7])  # zero_variance
    assert not bool(fleet[6])  # not all_zero


def test_dimension_stats_single_seed_not_zero_variance() -> None:
    """Single seed should not be flagged as zero_variance (trivially true)."""
    conn = duckdb.connect(":memory:")
    rel = _make_score_relation(
        conn,
        [
            (0, 100, 400.0, 0.6, 0.5, 0.4, 0.8, 0.7, 0.3),
        ],
    )
    result = dimension_stats(rel)
    rows = result.fetchall()
    for row in rows:
        assert not bool(row[7]), f"{row[0]} should not be zero_variance with 1 seed"


# --- temporal_change ---


def test_temporal_change_detects_growth() -> None:
    """Dimensions that grow over time are not static."""
    conn = duckdb.connect(":memory:")
    rel = _make_score_relation(
        conn,
        [
            (0, 24, 100.0, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1),
            (0, 48, 200.0, 0.3, 0.2, 0.3, 0.4, 0.5, 0.2),
            (0, 72, 400.0, 0.6, 0.5, 0.5, 0.7, 0.8, 0.4),
        ],
    )
    result = temporal_change(rel)
    rows = result.fetchall()
    for row in rows:
        assert not bool(row[3]), f"{row[0]} should not be static"


def test_temporal_change_detects_static() -> None:
    """Dimensions that don't change over time are flagged as static."""
    conn = duckdb.connect(":memory:")
    # expansion stays at 0.3 across all ticks
    rel = _make_score_relation(
        conn,
        [
            (0, 24, 100.0, 0.1, 0.1, 0.1, 0.1, 0.1, 0.3),
            (0, 48, 200.0, 0.3, 0.2, 0.3, 0.4, 0.5, 0.3),
            (0, 72, 400.0, 0.6, 0.5, 0.5, 0.7, 0.8, 0.3),
        ],
    )
    result = temporal_change(rel)
    rows = result.fetchall()
    expansion = next(r for r in rows if r[0] == "score_expansion")
    assert bool(expansion[3])  # static


# --- build_gap_report ---


def test_gap_report_no_gaps() -> None:
    """Report with healthy data has no gaps."""
    conn = duckdb.connect(":memory:")
    rel = _make_score_relation(
        conn,
        [
            (0, 24, 100.0, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1),
            (0, 72, 400.0, 0.6, 0.5, 0.5, 0.7, 0.8, 0.4),
            (1, 24, 120.0, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2),
            (1, 72, 500.0, 0.7, 0.6, 0.6, 0.8, 0.9, 0.5),
        ],
    )
    stats = dimension_stats(rel)
    temporal = temporal_change(rel)
    report = build_gap_report(stats, temporal)
    assert len(report["gaps"]) == 0
    assert "No gaps detected" in report["summary"]


def test_gap_report_flags_all_zero() -> None:
    """Report flags all-zero dimensions."""
    conn = duckdb.connect(":memory:")
    rel = _make_score_relation(
        conn,
        [
            (0, 24, 100.0, 0.1, 0.1, 0.1, 0.1, 0.1, 0.0),
            (0, 72, 400.0, 0.6, 0.5, 0.5, 0.7, 0.8, 0.0),
            (1, 24, 120.0, 0.2, 0.2, 0.2, 0.2, 0.2, 0.0),
            (1, 72, 500.0, 0.7, 0.6, 0.6, 0.8, 0.9, 0.0),
        ],
    )
    stats = dimension_stats(rel)
    temporal = temporal_change(rel)
    report = build_gap_report(stats, temporal)
    gap_dimensions = [g["dimension"] for g in report["gaps"]]
    assert "score_expansion" in gap_dimensions
    zero_gap = next(g for g in report["gaps"] if g["dimension"] == "score_expansion" and g["issue"] == "all_zero")
    assert "zero across all" in zero_gap["detail"]


def test_gap_report_json_serializable() -> None:
    """Report can be serialized to JSON."""
    import json as json_mod

    conn = duckdb.connect(":memory:")
    rel = _make_score_relation(
        conn,
        [
            (0, 100, 400.0, 0.6, 0.5, 0.4, 0.8, 0.7, 0.3),
            (1, 100, 600.0, 0.8, 0.7, 0.6, 0.9, 0.8, 0.5),
        ],
    )
    stats = dimension_stats(rel)
    temporal = temporal_change(rel)
    report = build_gap_report(stats, temporal)
    json_str = json_mod.dumps(report)
    parsed = json_mod.loads(json_str)
    assert "dimensions" in parsed
    assert "gaps" in parsed
    assert "summary" in parsed
