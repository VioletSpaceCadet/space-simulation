"""Tests for bottleneck majority-class baseline."""

from __future__ import annotations

import duckdb

from scripts.analysis.models.bottleneck_stub import (
    majority_class_evaluate,
    majority_class_train,
)


def test_majority_class_train_picks_most_common() -> None:
    """Majority class is the type with the most total ticks."""
    conn = duckdb.connect(":memory:")
    timeline = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 100, 'Healthy'),
            (0, 101, 120, 'StorageFull'),
            (1, 1, 200, 'Healthy')
        ) AS t(seed, tick_start, tick_end, bottleneck_type)
    """)
    result = majority_class_train(timeline)
    # Healthy: (100-1+1) + (200-1+1) = 100 + 200 = 300 ticks
    # StorageFull: (120-101+1) = 20 ticks
    assert result == "Healthy"


def test_majority_class_train_weighted_by_duration() -> None:
    """Longer spans count more than frequent short spans."""
    conn = duckdb.connect(":memory:")
    timeline = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 10, 'StorageFull'),
            (0, 11, 20, 'StorageFull'),
            (0, 21, 30, 'StorageFull'),
            (0, 31, 500, 'Healthy')
        ) AS t(seed, tick_start, tick_end, bottleneck_type)
    """)
    result = majority_class_train(timeline)
    # StorageFull: 3 spans x 10 ticks = 30 ticks
    # Healthy: 1 span x 470 ticks = 470 ticks
    assert result == "Healthy"


def test_majority_class_evaluate_perfect() -> None:
    """100% accuracy when all ticks are the predicted class."""
    conn = duckdb.connect(":memory:")
    timeline = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 100, 'Healthy'),
            (1, 1, 100, 'Healthy')
        ) AS t(seed, tick_start, tick_end, bottleneck_type)
    """)
    accuracy = majority_class_evaluate(timeline, "Healthy")
    assert accuracy == 1.0


def test_majority_class_evaluate_partial() -> None:
    """Partial accuracy when some ticks differ."""
    conn = duckdb.connect(":memory:")
    timeline = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 75, 'Healthy'),
            (0, 76, 100, 'StorageFull')
        ) AS t(seed, tick_start, tick_end, bottleneck_type)
    """)
    accuracy = majority_class_evaluate(timeline, "Healthy")
    # Healthy: 75 ticks, StorageFull: 25 ticks, total: 100
    assert abs(accuracy - 0.75) < 0.01


def test_majority_class_evaluate_zero() -> None:
    """0% accuracy when no ticks match the prediction."""
    conn = duckdb.connect(":memory:")
    timeline = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 100, 'StorageFull')
        ) AS t(seed, tick_start, tick_end, bottleneck_type)
    """)
    accuracy = majority_class_evaluate(timeline, "Healthy")
    assert accuracy == 0.0


def test_majority_class_train_empty() -> None:
    """Empty timeline returns default 'Healthy'."""
    conn = duckdb.connect(":memory:")
    timeline = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 100, 'Healthy')
        ) AS t(seed, tick_start, tick_end, bottleneck_type)
        WHERE 1 = 0
    """)
    result = majority_class_train(timeline)
    assert result == "Healthy"


def test_majority_class_evaluate_empty() -> None:
    """Empty timeline returns 0.0 accuracy."""
    conn = duckdb.connect(":memory:")
    timeline = conn.sql("""
        SELECT * FROM (VALUES
            (0, 1, 100, 'Healthy')
        ) AS t(seed, tick_start, tick_end, bottleneck_type)
        WHERE 1 = 0
    """)
    accuracy = majority_class_evaluate(timeline, "Healthy")
    assert accuracy == 0.0
