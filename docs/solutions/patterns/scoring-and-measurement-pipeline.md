---
title: "Scoring & Measurement Pipeline — Content-Driven Run Scoring with Full-Stack Integration"
category: patterns
date: 2026-04-01
tags: [scoring, sim_bench, measurement, optimization, cross-layer, content-driven, pipeline]
components: [sim_core, sim_bench, sim_daemon, ui_web, scripts/analysis]
tickets: [VIO-519, VIO-520, VIO-521, VIO-522, VIO-523, VIO-524, VIO-525, VIO-526, VIO-528, VIO-529]
prs: [373, 374, 375, 376, 377, 378, 379, 380, 381, 382]
---

## Problem

The simulation had no quantitative scoring system. Balance tuning relied on manual observation of metrics. There was no way to answer "is config A better than config B?" or "which autopilot parameters matter most?" — blocking automated optimization.

## Architecture: The Scoring Pipeline

The P0 project established a complete measurement-to-optimization loop across 5 layers:

```
content/scoring.json  →  sim_core::compute_run_score()  →  sim_bench Parquet export
                                                          →  sim_daemon /api/v1/score + SSE
                                                          →  Python ML pipeline (labels, gaps, optimize)
```

### Layer 1: Content-Driven Scoring Schema (VIO-519)

- `content/scoring.json`: 6 dimensions with weights/ceilings, 5 named thresholds, computation interval
- `ScoringConfig` loaded as part of `GameContent` — validated at content load time
- Dimensions are content-defined strings, not Rust enums. Adding a dimension = adding a JSON entry.
- Scale factor (2500.0) applied to weighted sum for human-readable composite scores

### Layer 2: Pure Scoring Function (VIO-521)

- `compute_run_score(metrics, state, content) → RunScore` — pure, deterministic, no IO
- Each dimension has a raw value function, normalized to [0,1] by ceiling, then weighted
- Six dimension formulas: industrial (throughput + assembler), research (tech fraction + scan data), economic (balance ratio + revenue), fleet (utilization + construction), efficiency (wear + power + storage), expansion (stations + fleet reach)
- Threshold resolution: highest threshold whose min_score <= composite

### Layer 3: AutopilotConfig Extraction (VIO-520)

- Behavioral parameters extracted from hardcoded values into `content/autopilot.json`
- 13 parameters covering refueling, storage, export, shipyard, power, crew
- `autopilot.*` override routing in sim_bench scenarios
- Critical: preserved production-tuned values (e.g., `refinery_threshold_kg=2000` from BALANCE.md, not the pre-tuning `500`)

### Layer 4: Cross-Layer Integration

**sim_bench** (VIO-522): Score computed alongside metrics at each interval. 8 Parquet columns (composite, 6 normalized dimensions, threshold string). Score stats in cross-seed summary.

**sim_daemon** (VIO-523): `GET /api/v1/score` endpoint with cached RunScore. Score computed every `computation_interval_ticks` in tick loop. `ScoreThresholdCrossed` SSE event on threshold transitions. AdvisorDigest extended with score + trend (Improving/Declining/Stable).

**Key pattern**: Shared `compute_metrics()` snapshot between metrics and score computation — hoisted above both blocks to avoid duplicate computation when intervals align.

**Event ID correctness**: Score threshold events must use `game_state.counters.next_event_id` (not `next_command_id`) to avoid ID collisions with the command system.

### Layer 5: Python Analysis Pipeline (VIO-524, VIO-525, VIO-528)

- `scoring_dimensions()`: reads pre-computed Parquet columns (not recomputing from raw metrics)
- `score_trajectory()`: per-tick score curves for trend analysis
- `score_distribution()`: cross-seed statistics per dimension
- `data_gaps.py`: flags zero-variance, all-zero, and temporally static dimensions
- `optimize_config.py`: grid search over AutopilotConfig parameters via `sim_bench compare`

### Layer 6: Evaluation Framework (VIO-526)

- `sim_bench compare` subcommand: runs same seeds with two configs, computes paired deltas
- Hand-implemented paired t-test with critical value lookup table (df 1-30, normal approximation above)
- `ComparisonReport` JSON: per-dimension deltas, composite t-test, per-seed breakdown
- Uses sample stddev (Bessel's correction) consistently across delta_summary and t-test

## Key Design Decisions

1. **Content-driven dimensions, not Rust enums** — new dimensions added in JSON, not code changes
2. **Pre-computed scores in Parquet** — Python reads scores directly, no formula duplication
3. **`legacy_final_score()` preserved** — backward-compatible alias for the old 4-component score
4. **DuckDB `decimal.Decimal` gotcha** — all Python test assertions must use `float()` casts on VALUES clause results
5. **Single-seed variance guard** — `STDDEV_SAMP` returns NULL for n=1, `COALESCE` to 0.0 falsely triggers zero_variance flag. Fixed with `CASE WHEN COUNT(*) < 2 THEN false` guard.
6. **Scoring calibration in CI** — `scoring_smoke.json` (5 seeds x 500 ticks) validates non-degenerate scores as blocking CI check. Reads from `summary.json` (has score metrics), not `batch_summary.json` (only has MetricsSnapshot fields).

## Prevention / Best Practices

- When adding a new scoring dimension: add to `content/scoring.json`, add a `compute_*` function in `scoring.rs`, add Parquet column in `parquet_writer.rs`, add to summary dimension list in `summary.rs`
- When adding daemon events: use `game_state.counters.next_event_id`, not any other counter
- When writing Python tests against DuckDB: always `float()` cast VALUES results before arithmetic
- When validating batch outputs: check which JSON file contains the data you need (`summary.json` for scores, `batch_summary.json` for raw metrics)

## Cross-References

- `docs/BALANCE.md` — balance analysis findings that informed parameter extraction
- `content/scoring.json` — authoritative scoring schema
- `content/autopilot.json` — baseline behavioral parameters
- `scenarios/scoring_calibration.json` — full calibration scenario (20 seeds x 2000 ticks)
- `scripts/analysis/optimize_config.py` — grid search orchestration
