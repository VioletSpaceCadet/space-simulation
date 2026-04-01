"""Tests for grid search optimization scaffolding."""

from __future__ import annotations

from scripts.analysis.optimize_config import generate_variants, rank_variants

# --- generate_variants ---


def test_generate_variants_single_param() -> None:
    """Single parameter produces one variant per value."""
    baseline = {"version": "v1", "slag_jettison_pct": 0.75}
    grid = {"slag_jettison_pct": [0.5, 0.75, 0.9]}
    variants = generate_variants(baseline, grid)
    assert len(variants) == 3
    assert variants[0][0]["slag_jettison_pct"] == 0.5
    assert variants[1][0]["slag_jettison_pct"] == 0.75
    assert variants[2][0]["slag_jettison_pct"] == 0.9


def test_generate_variants_cartesian_product() -> None:
    """Multiple parameters produce cartesian product."""
    baseline = {"version": "v1", "a": 1, "b": "x"}
    grid = {"a": [1, 2], "b": ["x", "y"]}
    variants = generate_variants(baseline, grid)
    assert len(variants) == 4  # 2 x 2
    overrides = [v[1] for v in variants]
    assert {"a": 1, "b": "x"} in overrides
    assert {"a": 1, "b": "y"} in overrides
    assert {"a": 2, "b": "x"} in overrides
    assert {"a": 2, "b": "y"} in overrides


def test_generate_variants_preserves_baseline() -> None:
    """Unmodified baseline fields are preserved in variants."""
    baseline = {"version": "v1", "a": 1, "b": 2, "c": 3}
    grid = {"a": [10]}
    variants = generate_variants(baseline, grid)
    assert len(variants) == 1
    config, overrides = variants[0]
    assert config["a"] == 10
    assert config["b"] == 2
    assert config["c"] == 3
    assert overrides == {"a": 10}


def test_generate_variants_empty_grid() -> None:
    """Empty grid produces single variant (baseline unchanged)."""
    baseline = {"version": "v1", "a": 1}
    grid: dict[str, list[object]] = {}
    variants = generate_variants(baseline, grid)
    assert len(variants) == 1
    assert variants[0][0] == baseline
    assert variants[0][1] == {}


def test_generate_variants_overrides_tracked() -> None:
    """Each variant tracks which parameters were overridden."""
    baseline = {"version": "v1", "a": 1, "b": 2}
    grid = {"a": [10, 20]}
    variants = generate_variants(baseline, grid)
    assert variants[0][1] == {"a": 10}
    assert variants[1][1] == {"a": 20}


# --- rank_variants ---


def test_rank_variants_by_composite() -> None:
    """Variants ranked by composite delta descending."""
    results = [
        (
            "low",
            {"a": 1},
            {
                "composite_delta": {"mean": -10.0, "stddev": 5.0},
                "composite_t_test": {"significant_at_05": False},
                "dimension_deltas": [],
            },
        ),
        (
            "high",
            {"a": 2},
            {
                "composite_delta": {"mean": 50.0, "stddev": 3.0},
                "composite_t_test": {"significant_at_05": True},
                "dimension_deltas": [],
            },
        ),
        (
            "mid",
            {"a": 3},
            {
                "composite_delta": {"mean": 20.0, "stddev": 8.0},
                "composite_t_test": {"significant_at_05": False},
                "dimension_deltas": [],
            },
        ),
    ]
    ranked = rank_variants(results)
    assert len(ranked) == 3
    assert ranked[0]["label"] == "high"
    assert ranked[0]["rank"] == 1
    assert ranked[1]["label"] == "mid"
    assert ranked[2]["label"] == "low"


def test_rank_variants_includes_significance() -> None:
    """Significance flag is preserved in ranking."""
    results = [
        (
            "sig",
            {"a": 1},
            {
                "composite_delta": {"mean": 10.0, "stddev": 1.0},
                "composite_t_test": {"significant_at_05": True},
                "dimension_deltas": [],
            },
        ),
    ]
    ranked = rank_variants(results)
    assert ranked[0]["significant"] is True


def test_rank_variants_empty() -> None:
    """Empty results list returns empty ranking."""
    ranked = rank_variants([])
    assert ranked == []


def test_rank_variants_dimension_deltas() -> None:
    """Dimension deltas are extracted from report."""
    results = [
        (
            "test",
            {"a": 1},
            {
                "composite_delta": {"mean": 10.0, "stddev": 1.0},
                "composite_t_test": {"significant_at_05": False},
                "dimension_deltas": [
                    {"dimension_id": "industrial_output", "delta": {"mean": 0.05}},
                    {"dimension_id": "efficiency", "delta": {"mean": -0.02}},
                ],
            },
        ),
    ]
    ranked = rank_variants(results)
    assert ranked[0]["dimension_deltas"]["industrial_output"] == 0.05
    assert ranked[0]["dimension_deltas"]["efficiency"] == -0.02
