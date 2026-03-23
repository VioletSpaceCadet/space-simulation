"""Tests for scoring mean-predictor baseline."""

from __future__ import annotations

import duckdb

from scripts.analysis.models.scoring_stub import (
    mean_predict_evaluate,
    mean_predict_train,
)


def test_mean_predict_train_returns_average() -> None:
    """Mean prediction is the average of all scores."""
    conn = duckdb.connect(":memory:")
    scores = conn.sql("""
        SELECT * FROM (VALUES
            (0, 0.4, 1000),
            (1, 0.6, 1000),
            (2, 0.8, 1000)
        ) AS t(seed, score, final_tick)
    """)
    mean = mean_predict_train(scores)
    assert abs(mean - 0.6) < 0.01


def test_mean_predict_train_single_value() -> None:
    """Single score returns that value."""
    conn = duckdb.connect(":memory:")
    scores = conn.sql("""
        SELECT * FROM (VALUES
            (0, 0.5, 1000)
        ) AS t(seed, score, final_tick)
    """)
    mean = mean_predict_train(scores)
    assert abs(mean - 0.5) < 0.01


def test_mean_predict_evaluate_zero_mse() -> None:
    """MSE is zero when all scores equal the prediction."""
    conn = duckdb.connect(":memory:")
    scores = conn.sql("""
        SELECT * FROM (VALUES
            (0, 0.5, 1000),
            (1, 0.5, 1000)
        ) AS t(seed, score, final_tick)
    """)
    mse = mean_predict_evaluate(scores, 0.5)
    assert abs(mse) < 1e-10


def test_mean_predict_evaluate_nonzero_mse() -> None:
    """MSE computed correctly for varying scores."""
    conn = duckdb.connect(":memory:")
    scores = conn.sql("""
        SELECT * FROM (VALUES
            (0, 0.0, 1000),
            (1, 1.0, 1000)
        ) AS t(seed, score, final_tick)
    """)
    # Predicting 0.5: MSE = ((0-0.5)^2 + (1-0.5)^2) / 2 = 0.25
    mse = mean_predict_evaluate(scores, 0.5)
    assert abs(mse - 0.25) < 0.01


def test_mean_predict_evaluate_poor_prediction() -> None:
    """Worse prediction yields higher MSE."""
    conn = duckdb.connect(":memory:")
    scores = conn.sql("""
        SELECT * FROM (VALUES
            (0, 0.2, 1000),
            (1, 0.3, 1000),
            (2, 0.25, 1000)
        ) AS t(seed, score, final_tick)
    """)
    good_mse = mean_predict_evaluate(scores, 0.25)
    bad_mse = mean_predict_evaluate(scores, 1.0)
    assert bad_mse > good_mse


def test_mean_predict_train_empty() -> None:
    """Empty scores returns 0.0."""
    conn = duckdb.connect(":memory:")
    scores = conn.sql("""
        SELECT * FROM (VALUES
            (0, 0.5, 1000)
        ) AS t(seed, score, final_tick)
        WHERE 1 = 0
    """)
    mean = mean_predict_train(scores)
    assert mean == 0.0
