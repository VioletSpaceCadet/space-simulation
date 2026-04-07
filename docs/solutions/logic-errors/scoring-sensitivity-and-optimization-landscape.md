---
title: "Scoring system sensitivity gaps and flat optimization landscape"
category: logic-errors
date: 2026-04-06
tags:
  - scoring
  - optimization
  - strategy-config
  - sim_bench
  - bayesian-optimization
  - regression-testing
problem_type: architecture-decision
component: sim_core/scoring, sim_core/metrics, sim_bench, scripts/analysis
severity: medium
related:
  - docs/solutions/patterns/scoring-and-measurement-pipeline.md
  - docs/solutions/patterns/p6-ai-optimization-architectural-patterns.md
---

## Problem

When validating the Bayesian optimization pipeline (VIO-614), two scoring system gaps surfaced:

1. **dev_advanced_state produces zero variance** — all 30 optimization trials scored exactly 597.15 regardless of strategy parameters. The scoring system is completely insensitive to strategy changes when starting from an advanced state.

2. **Flat optimization landscape** — even with ground_start, the scoring range is narrow. At 50k ticks across 50 trials, the range was only 672-689 (2.5% spread). The optimizer found consistent but marginal improvement (+0.9%).

3. **MetricsSnapshot lacked progression data** — `game_phase` and `milestones_completed` were not tracked, so progression regression tests couldn't validate phase gates through sim_bench output.

## Root cause

**Why dev_advanced_state is invariant:** The advanced state starts with enough infrastructure (stations, ships, modules, research) that the autopilot's strategy priorities don't meaningfully change outcomes within the scoring window. The scoring dimensions (industrial_output, research_progress, economic_health, fleet_operations, efficiency, expansion) are dominated by the starting infrastructure, not by how aggressively the autopilot pursues each concern.

**Why the landscape is flat:** The autopilot's core decision logic (mine/survey/refine/export pipeline) is robust to parameter variation. Most parameter combinations produce similar outcomes because the pipeline's bottlenecks are structural (ore availability, refinery throughput, tech tree gates) rather than priority-driven. Strategy parameters modulate *allocation* within a narrow band, not the fundamental production rate.

**Why MetricsSnapshot had no progression:** It was designed for economic/operational metrics. Progression state (phase, milestones) lives on `GameState.progression` and wasn't surfaced through the metrics pipeline.

## Solution

### 1. Added progression fields to MetricsSnapshot (VIO-611)

```rust
// In MetricsSnapshot:
pub milestones_completed: u32,
pub game_phase: u32,  // ordinal: 0=Startup, 1=Orbital, 2=Industrial, ...
```

Added `#[repr(u32)]` to `GamePhase` enum to make the ordinal contract explicit. These fields flow through `fixed_field_values()` → `fixed_field_descriptors()` → batch_summary.json → Parquet automatically.

**8 construction sites** needed updating (test helpers in scoring.rs, alerts.rs, analytics.rs, run_result.rs, parquet_writer.rs, summary.rs).

### 2. Progression CI gate (VIO-611)

`progression_full_arc` scenario (30k ticks, 5 seeds, ground_start) with `validate_progression_full_arc.py` that reads per-seed `run_result.json` files and gates on:
- 80%+ seeds reach Orbital phase
- 80%+ seeds complete 4+ milestones
- Zero collapses

CI impact: negligible (30k ticks x 5 seeds completes in <1s release mode).

### 3. Optimization validation scenarios (VIO-614)

`strategy_default.json` and `strategy_optimized.json` (50k ticks, 10 seeds, ground_start) with `validate_strategy_comparison.py` comparing composite scores and dimension regressions.

**Key optimizer findings (50-trial TPE, 50k ticks):**
- Best config: +0.9% composite, 70% lower variance (stddev 8.76 → 2.61)
- High maintenance (0.904), propellant (0.941), export (0.789) priorities
- Low fleet_expansion (0.146) — consolidation over expansion at long horizons
- 35/50 trials beat default, but improvement range is narrow

### 4. Choosing the right base state for optimization

| Base state | Ticks | Outcome |
|-----------|-------|---------|
| dev_advanced_state | any | Zero variance — useless for optimization |
| ground_start, 20k | 20k | Variance appears but optimizer advantage is noise |
| ground_start, 50k | 50k | Consistent improvement, low variance |

**Rule: always use ground_start with 50k+ ticks for strategy optimization.** dev_advanced_state is only useful for smoke tests and non-strategy validation.

## Prevention

**When adding new scoring dimensions or modifying the scoring pipeline:**
- Run the optimizer (even 10 trials) against the new scoring to verify the landscape has meaningful gradients
- If all trials produce the same score, the dimension is insensitive to strategy — either the dimension is dominated by starting conditions, or it needs rescaling

**When choosing base states for optimization/comparison scenarios:**
- Use ground_start, never dev_advanced_state
- Use 50k+ ticks for meaningful differentiation
- Use 10+ seeds for statistical confidence (3 seeds showed improvement that disappeared at 10)

**When adding fields to MetricsSnapshot:**
- Update `fixed_field_values()` AND `fixed_field_descriptors()` (parallel arrays)
- Update all test helper construction sites (search for last field name in the struct)
- The `test_empty_state_all_zeros` test validates exhaustive coverage — add assertions for new fields
