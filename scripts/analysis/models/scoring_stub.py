"""Mean-predictor baseline for economy scoring.

Placeholder for a real regression model. Proves the pipeline from
Parquet -> DuckDB -> features -> labels -> model works end-to-end.

The mean predictor predicts the average final_score for every seed.
MSE of a mean predictor equals the population variance of the target.
No ML framework required.
"""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import duckdb


def mean_predict_train(scores: duckdb.DuckDBPyRelation) -> float:
    """Compute the mean score as the baseline prediction.

    Args:
        scores: Output from final_score() with columns (seed, score, final_tick).

    Returns:
        The mean score value.
    """
    result = scores.aggregate("AVG(score) AS mean_score")
    row = result.fetchone()
    if row is None or row[0] is None:
        return 0.0
    return float(row[0])


def mean_predict_evaluate(
    scores: duckdb.DuckDBPyRelation,
    predicted_mean: float,
) -> float:
    """Compute MSE of always predicting the given mean.

    Args:
        scores: Scoring relation with 'score' column.
        predicted_mean: The constant prediction value.

    Returns:
        Mean squared error.
    """
    result = scores.query(
        "s",
        f"""
        SELECT COALESCE(
            AVG(POWER(score - {predicted_mean}, 2)),
            0.0
        ) AS mse
        FROM s
        """,  # noqa: S608
    )
    row = result.fetchone()
    return float(row[0]) if row is not None else 0.0


def main() -> None:
    """CLI: train and evaluate mean-predictor scoring baseline."""
    if len(sys.argv) < 2:
        print(
            "Usage: python3 scripts/analysis/models/scoring_stub.py <run_dir>",
            file=sys.stderr,
        )
        sys.exit(1)

    from scripts.analysis.labels import final_score
    from scripts.analysis.load_run import load_run

    run_dir = sys.argv[1]
    rel = load_run(run_dir)
    scores = final_score(rel)

    # Split by seed: seed % 3 = 0 -> test, rest -> train
    train = scores.filter("seed % 3 != 0")
    test = scores.filter("seed % 3 = 0")

    mean_pred = mean_predict_train(train)
    train_mse = mean_predict_evaluate(train, mean_pred)
    test_mse = mean_predict_evaluate(test, mean_pred)

    print("=== Economy Scoring (Mean-Predictor Baseline) ===")
    print(f"Mean prediction: {mean_pred:.4f}")
    print(f"Train MSE: {train_mse:.6f}")
    print(f"Test MSE:  {test_mse:.6f}")


if __name__ == "__main__":
    main()
