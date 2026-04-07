"""Tests for Bayesian optimization over StrategyConfig parameters."""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import MagicMock

from scripts.analysis.optimize_strategy import (
    export_best_config,
    save_history,
    suggest_strategy_params,
)

# --- suggest_strategy_params ---


def test_suggest_params_returns_all_expected_keys() -> None:
    """All strategy parameters are present in suggestions."""
    trial = MagicMock()
    trial.suggest_float = MagicMock(return_value=0.5)
    trial.suggest_int = MagicMock(return_value=3)

    params = suggest_strategy_params(trial)

    # Priority weights (8)
    assert "strategy.priorities.mining" in params
    assert "strategy.priorities.survey" in params
    assert "strategy.priorities.deep_scan" in params
    assert "strategy.priorities.research" in params
    assert "strategy.priorities.maintenance" in params
    assert "strategy.priorities.export" in params
    assert "strategy.priorities.propellant" in params
    assert "strategy.priorities.fleet_expansion" in params

    # Thresholds
    assert "strategy.fleet_size_target" in params
    assert "strategy.volatile_threshold_kg" in params
    assert "strategy.lh2_threshold_kg" in params
    assert "strategy.slag_jettison_pct" in params
    assert "strategy.export_batch_size_kg" in params
    assert "strategy.budget_cap_fraction" in params
    assert "strategy.refuel_threshold_pct" in params


def test_suggest_params_uses_dotted_strategy_keys() -> None:
    """All keys use strategy.* prefix for sim_bench overrides."""
    trial = MagicMock()
    trial.suggest_float = MagicMock(return_value=0.5)
    trial.suggest_int = MagicMock(return_value=3)

    params = suggest_strategy_params(trial)
    for key in params:
        assert key.startswith("strategy."), f"Key {key} missing strategy. prefix"


def test_suggest_params_count() -> None:
    """Correct number of parameters in the search space."""
    trial = MagicMock()
    trial.suggest_float = MagicMock(return_value=0.5)
    trial.suggest_int = MagicMock(return_value=3)

    params = suggest_strategy_params(trial)
    # 8 priorities + 10 thresholds = 18 total
    assert len(params) == 18


# --- export_best_config ---


def test_export_best_config_structure(tmp_path: Any) -> None:
    """Exported config has correct structure."""
    study = MagicMock()
    study.best_trial.params = {
        "priorities.mining": 0.8,
        "priorities.survey": 0.6,
        "priorities.deep_scan": 0.3,
        "priorities.research": 0.7,
        "priorities.maintenance": 0.9,
        "priorities.export": 0.5,
        "priorities.propellant": 0.8,
        "priorities.fleet_expansion": 0.4,
        "fleet_size_target": 5,
        "volatile_threshold_kg": 800.0,
        "slag_jettison_pct": 0.85,
    }
    study.best_trial.value = 950.0

    export_path = str(tmp_path / "strategy_optimized.json")
    export_best_config(study, export_path)

    with open(export_path) as f:
        config = json.load(f)

    assert config["version"] == "strategy-v2-optimized"
    assert config["mode"] == "Balanced"
    assert config["priorities"]["mining"] == 0.8
    assert config["priorities"]["survey"] == 0.6
    assert config["fleet_size_target"] == 5
    assert config["slag_jettison_pct"] == 0.85


def test_export_rounds_floats(tmp_path: Any) -> None:
    """Float values are rounded to avoid noise."""
    study = MagicMock()
    study.best_trial.params = {
        "priorities.mining": 0.8123456789,
        "volatile_threshold_kg": 543.21098765,
    }
    study.best_trial.value = 900.0

    export_path = str(tmp_path / "config.json")
    export_best_config(study, export_path)

    with open(export_path) as f:
        config = json.load(f)

    assert config["priorities"]["mining"] == 0.812
    assert config["volatile_threshold_kg"] == 543.211


# --- save_history ---


def test_save_history_structure(tmp_path: Any) -> None:
    """History JSON has expected structure."""
    study = MagicMock()
    trial_a = MagicMock()
    trial_a.number = 0
    trial_a.value = 800.0
    trial_a.params = {"priorities.mining": 0.5}
    trial_a.state.name = "COMPLETE"

    trial_b = MagicMock()
    trial_b.number = 1
    trial_b.value = 900.0
    trial_b.params = {"priorities.mining": 0.8}
    trial_b.state.name = "COMPLETE"

    trial_pruned = MagicMock()
    trial_pruned.number = 2
    trial_pruned.state.name = "PRUNED"

    study.trials = [trial_a, trial_b, trial_pruned]
    study.best_value = 900.0
    study.best_params = {"priorities.mining": 0.8}
    study.best_trial = trial_b

    save_history(study, str(tmp_path))

    with open(tmp_path / "optimization_history.json") as f:
        history = json.load(f)

    assert history["best_score"] == 900.0
    assert history["total_trials"] == 3
    assert history["completed_trials"] == 2
    # Sorted by score descending
    assert history["trials"][0]["score"] == 900.0
    assert history["trials"][1]["score"] == 800.0


def test_save_history_empty(tmp_path: Any) -> None:
    """Empty study produces valid history."""
    study = MagicMock()
    study.trials = []
    study.best_trial = None
    study.best_value = None
    study.best_params = None

    save_history(study, str(tmp_path))

    with open(tmp_path / "optimization_history.json") as f:
        history = json.load(f)

    assert history["completed_trials"] == 0
    assert history["trials"] == []
