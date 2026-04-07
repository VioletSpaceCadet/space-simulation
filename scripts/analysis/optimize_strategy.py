"""Bayesian optimization over StrategyConfig parameters via sim_bench.

Uses optuna to search the strategy parameter space, maximizing composite
score across multiple seeds. Evolves VIO-528's grid search into proper
Bayesian optimization with Gaussian Process surrogate models.

Usage:
    python3 scripts/analysis/optimize_strategy.py \\
        --scenario scenarios/scoring_smoke.json \\
        --sim-bench target/release/sim_bench \\
        --output-dir runs/bayesian_opt \\
        --trials 50 \\
        --seeds 5

    # Export best config:
    python3 scripts/analysis/optimize_strategy.py \\
        --scenario scenarios/scoring_smoke.json \\
        --sim-bench target/release/sim_bench \\
        --output-dir runs/bayesian_opt \\
        --trials 50 \\
        --export content/strategy_optimized.json
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

try:
    import optuna
except ImportError:
    optuna = None  # type: ignore[assignment]


# ---------------------------------------------------------------------------
# Parameter space definition
# ---------------------------------------------------------------------------


def suggest_strategy_params(trial: Any) -> dict[str, Any]:
    """Define the StrategyConfig parameter space for optuna.

    Returns a dict of strategy override keys (dotted notation for sim_bench)
    and their suggested values.
    """
    params: dict[str, Any] = {}

    # Priority weights [0.0, 1.0]
    params["strategy.priorities.mining"] = trial.suggest_float("priorities.mining", 0.1, 1.0)
    params["strategy.priorities.survey"] = trial.suggest_float("priorities.survey", 0.1, 1.0)
    params["strategy.priorities.deep_scan"] = trial.suggest_float("priorities.deep_scan", 0.0, 0.8)
    params["strategy.priorities.research"] = trial.suggest_float("priorities.research", 0.1, 1.0)
    params["strategy.priorities.maintenance"] = trial.suggest_float("priorities.maintenance", 0.3, 1.0)
    params["strategy.priorities.export"] = trial.suggest_float("priorities.export", 0.1, 1.0)
    params["strategy.priorities.propellant"] = trial.suggest_float("priorities.propellant", 0.3, 1.0)
    params["strategy.priorities.fleet_expansion"] = trial.suggest_float("priorities.fleet_expansion", 0.1, 1.0)

    # Fleet size target
    params["strategy.fleet_size_target"] = trial.suggest_int("fleet_size_target", 1, 8)

    # Operational thresholds
    params["strategy.volatile_threshold_kg"] = trial.suggest_float("volatile_threshold_kg", 100.0, 2000.0)
    params["strategy.lh2_threshold_kg"] = trial.suggest_float("lh2_threshold_kg", 1000.0, 20000.0)
    params["strategy.lh2_abundant_multiplier"] = trial.suggest_float("lh2_abundant_multiplier", 1.5, 5.0)
    params["strategy.refinery_threshold_kg"] = trial.suggest_float("refinery_threshold_kg", 500.0, 5000.0)
    params["strategy.slag_jettison_pct"] = trial.suggest_float("slag_jettison_pct", 0.5, 0.95)
    params["strategy.export_batch_size_kg"] = trial.suggest_float("export_batch_size_kg", 100.0, 2000.0)
    params["strategy.export_min_revenue"] = trial.suggest_float("export_min_revenue", 100.0, 5000.0)
    params["strategy.budget_cap_fraction"] = trial.suggest_float("budget_cap_fraction", 0.01, 0.15)
    params["strategy.refuel_threshold_pct"] = trial.suggest_float("refuel_threshold_pct", 0.5, 0.95)

    return params


# ---------------------------------------------------------------------------
# Objective function
# ---------------------------------------------------------------------------


def run_trial(
    sim_bench_path: str,
    base_scenario_path: str,
    overrides: dict[str, Any],
    output_dir: str,
    trial_label: str,
) -> float | None:
    """Run sim_bench with strategy overrides and return composite score.

    Creates a temporary scenario file with the overrides applied, runs
    sim_bench, and parses the composite score from summary.json.
    """
    # Load base scenario and add overrides
    with open(base_scenario_path) as f:
        scenario = json.load(f)

    scenario["name"] = trial_label
    scenario["overrides"] = {**scenario.get("overrides", {}), **overrides}

    trial_dir = Path(output_dir) / trial_label
    trial_dir.mkdir(parents=True, exist_ok=True)

    # Write temporary scenario
    scenario_path = trial_dir / "scenario.json"
    scenario_path.write_text(json.dumps(scenario, indent=2))

    try:
        result = subprocess.run(
            [
                sim_bench_path,
                "run",
                "--scenario",
                str(scenario_path),
                "--output-dir",
                str(trial_dir),
            ],
            capture_output=True,
            text=True,
            timeout=600,
        )
    except subprocess.TimeoutExpired:
        print(f"  TIMEOUT: {trial_label}", file=sys.stderr)
        return None

    if result.returncode != 0:
        print(f"  FAILED: {trial_label}", file=sys.stderr)
        print(f"    {result.stderr[:200]}", file=sys.stderr)
        return None

    # Find summary.json and extract composite score
    summary_files = list(trial_dir.glob("*/summary.json"))
    if not summary_files:
        print(f"  No summary found for {trial_label}", file=sys.stderr)
        return None

    with open(summary_files[0]) as f:
        summary = json.load(f)

    for metric in summary.get("metrics", []):
        if metric["name"] == "score_composite":
            return float(metric["mean"])

    print(f"  No score_composite in summary for {trial_label}", file=sys.stderr)
    return None


def create_objective(
    sim_bench_path: str,
    base_scenario_path: str,
    output_dir: str,
) -> Any:
    """Create an optuna objective function closure."""

    def objective(trial: Any) -> float:
        overrides = suggest_strategy_params(trial)
        trial_label = f"trial_{trial.number:04d}"

        score = run_trial(
            sim_bench_path,
            base_scenario_path,
            overrides,
            output_dir,
            trial_label,
        )

        if score is None:
            raise optuna.TrialPruned()

        return score

    return objective


# ---------------------------------------------------------------------------
# Results export
# ---------------------------------------------------------------------------


def export_best_config(study: Any, export_path: str) -> None:
    """Export the best trial's parameters as a strategy config JSON."""
    best = study.best_trial

    # Reconstruct strategy config from trial params
    config: dict[str, Any] = {
        "version": "strategy-v2-optimized",
        "mode": "Balanced",
        "priorities": {},
    }

    priority_keys = [
        "mining",
        "survey",
        "deep_scan",
        "research",
        "maintenance",
        "export",
        "propellant",
        "fleet_expansion",
    ]
    threshold_keys = [
        "fleet_size_target",
        "volatile_threshold_kg",
        "lh2_threshold_kg",
        "lh2_abundant_multiplier",
        "refinery_threshold_kg",
        "slag_jettison_pct",
        "export_batch_size_kg",
        "export_min_revenue",
        "budget_cap_fraction",
        "refuel_threshold_pct",
    ]

    for key in priority_keys:
        param_name = f"priorities.{key}"
        if param_name in best.params:
            config["priorities"][key] = round(best.params[param_name], 3)

    for key in threshold_keys:
        if key in best.params:
            value = best.params[key]
            if isinstance(value, float):
                config[key] = round(value, 4)
            else:
                config[key] = value

    Path(export_path).parent.mkdir(parents=True, exist_ok=True)
    with open(export_path, "w") as f:
        json.dump(config, f, indent=2)
    print(f"\nBest config exported to: {export_path}")
    print(f"  Score: {best.value:.2f}")
    print(f"  Params: {json.dumps(best.params, indent=2)}")


def save_history(study: Any, output_dir: str) -> None:
    """Save optimization history as JSON for knowledge system."""
    history = []
    for trial in study.trials:
        if trial.state.name != "COMPLETE":
            continue
        history.append(
            {
                "trial": trial.number,
                "score": trial.value,
                "params": trial.params,
            }
        )

    history.sort(key=lambda t: t["score"] or 0, reverse=True)

    history_path = Path(output_dir) / "optimization_history.json"
    with open(history_path, "w") as f:
        json.dump(
            {
                "best_score": study.best_value if study.best_trial else None,
                "best_params": study.best_params if study.best_trial else None,
                "total_trials": len(study.trials),
                "completed_trials": len(history),
                "trials": history,
            },
            f,
            indent=2,
        )
    print(f"History written to: {history_path}")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    """CLI entry point."""
    import argparse

    if optuna is None:
        print(
            "optuna is required: pip install optuna",
            file=sys.stderr,
        )
        sys.exit(1)

    parser = argparse.ArgumentParser(description="Bayesian optimization over StrategyConfig parameters")
    parser.add_argument("--scenario", required=True, help="Base sim_bench scenario JSON")
    parser.add_argument(
        "--sim-bench",
        default="target/release/sim_bench",
        help="Path to sim_bench binary",
    )
    parser.add_argument(
        "--output-dir",
        default="runs/bayesian_opt",
        help="Output directory for trials",
    )
    parser.add_argument("--trials", type=int, default=50, help="Number of optimization trials")
    parser.add_argument(
        "--export",
        help="Export best config to this path (e.g., content/strategy_optimized.json)",
    )
    args = parser.parse_args()

    # Suppress optuna's verbose logging
    optuna.logging.set_verbosity(optuna.logging.WARNING)

    study = optuna.create_study(
        direction="maximize",
        study_name="strategy_optimization",
        sampler=optuna.samplers.TPESampler(seed=42),
    )

    objective = create_objective(args.sim_bench, args.scenario, args.output_dir)

    print(f"Starting Bayesian optimization: {args.trials} trials")
    print(f"  Scenario: {args.scenario}")
    print(f"  Output: {args.output_dir}")

    study.optimize(objective, n_trials=args.trials)

    # Print results
    print("\n=== Optimization Complete ===")
    print(f"Best score: {study.best_value:.2f}")
    print(f"Best params: {json.dumps(study.best_params, indent=2)}")

    save_history(study, args.output_dir)

    if args.export:
        export_best_config(study, args.export)


if __name__ == "__main__":
    main()
