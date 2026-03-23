"""Majority-class baseline for bottleneck classification.

Placeholder for a real classifier (XGBoost/LightGBM). Proves the pipeline
from Parquet -> DuckDB -> features -> labels -> model works end-to-end.

The majority-class baseline predicts the most common bottleneck type
(weighted by tick duration) for every tick. No ML framework required.
"""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import duckdb


def majority_class_train(timeline: duckdb.DuckDBPyRelation) -> str:
    """Find the majority bottleneck class weighted by tick span duration.

    Args:
        timeline: Output from bottleneck_timeline() with columns
            (seed, tick_start, tick_end, bottleneck_type).

    Returns:
        The most common bottleneck_type by total tick count.
    """
    result = timeline.query(
        "t",
        """
        SELECT bottleneck_type,
            SUM(tick_end - tick_start + 1) AS total_ticks
        FROM t
        GROUP BY bottleneck_type
        ORDER BY total_ticks DESC
        LIMIT 1
        """,
    )
    row = result.fetchone()
    if row is None:
        return "Healthy"
    return str(row[0])


def majority_class_evaluate(
    timeline: duckdb.DuckDBPyRelation,
    predicted_class: str,
) -> float:
    """Compute accuracy of always predicting the given class.

    Args:
        timeline: Bottleneck timeline relation.
        predicted_class: The class to predict for every tick.

    Returns:
        Accuracy as a float in [0.0, 1.0].
    """
    result = timeline.query(
        "t",
        f"""
        WITH totals AS (
            SELECT
                SUM(tick_end - tick_start + 1) AS total_ticks,
                SUM(CASE WHEN bottleneck_type = '{predicted_class}'
                    THEN tick_end - tick_start + 1 ELSE 0 END) AS correct_ticks
            FROM t
        )
        SELECT CASE WHEN total_ticks > 0
            THEN CAST(correct_ticks AS DOUBLE) / total_ticks
            ELSE 0.0
        END AS accuracy
        FROM totals
        """,  # noqa: S608
    )
    row = result.fetchone()
    return float(row[0]) if row is not None else 0.0


def main() -> None:
    """CLI: train and evaluate bottleneck majority-class baseline."""
    if len(sys.argv) < 2:
        print(
            "Usage: python3 scripts/analysis/models/bottleneck_stub.py <run_dir>",
            file=sys.stderr,
        )
        sys.exit(1)

    from scripts.analysis.features import add_all_features
    from scripts.analysis.labels import bottleneck_timeline
    from scripts.analysis.load_run import load_run

    run_dir = sys.argv[1]
    rel = load_run(run_dir)
    rel = add_all_features(rel)
    timeline = bottleneck_timeline(rel)

    # Split by seed: seed % 3 = 0 -> test, rest -> train
    train = timeline.filter("seed % 3 != 0")
    test = timeline.filter("seed % 3 = 0")

    majority = majority_class_train(train)
    train_acc = majority_class_evaluate(train, majority)
    test_acc = majority_class_evaluate(test, majority)

    print("=== Bottleneck Classification (Majority-Class Baseline) ===")
    print(f"Majority class: {majority}")
    print(f"Train accuracy: {train_acc:.3f}")
    print(f"Test accuracy:  {test_acc:.3f}")


if __name__ == "__main__":
    main()
