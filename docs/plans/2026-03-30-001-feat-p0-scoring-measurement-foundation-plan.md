---
title: "feat: P0 — Scoring & Measurement Foundation"
type: feat
status: active
date: 2026-03-30
origin: docs/plans/2026-03-28-003-feat-game-progression-system-plan.md
---

# P0: Scoring & Measurement Foundation

## Overview

The measurement infrastructure that every subsequent progression project builds on. Multi-dimensional run scoring across 6 dimensions, exported in Parquet/CSV, displayed in UI, and used by sim_bench for cross-seed comparison. Plus the AI evaluation loop: baseline AutopilotConfig extraction, decision quality measurement, optimization scaffolding, and data gap detection.

**The complete loop this project establishes:**

```
sim_core (tick) → MetricsSnapshot → compute_run_score() → RunScore
     ↓                                      ↓
  sim_bench (Parquet export)          sim_daemon (API + SSE)
     ↓                                      ↓
  scripts/analysis (ML pipeline)       UI score panel
     ↓
  labels.py (scoring dimensions from Parquet)
     ↓
  data gap detection (coverage analysis)
     ↓
  optimization scaffolding (grid search over AutopilotConfig)
     ↓
  ranked configs → better AutopilotConfig → sim_core (repeat)
```

This is the foundational measurement-to-optimization cycle. Every P1-P7 project extends it — adding new scoring signals, new AutopilotConfig parameters, new decision dimensions. By P6 (AI Intelligence), this loop has been running since P0 and the formal optimization is a capstone, not a cold start.

**Why it's first:** You can't tune what you can't measure. Every change after this — starting state, tech tree, satellites, stations — can be quantitatively evaluated: "Did this change improve score by X% across 100 seeds?"

## Problem Statement

The simulation currently has no unified quality metric. Analysis infrastructure exists in pieces:

| What Exists | Gap |
|---|---|
| `MetricsSnapshot` with 50+ fields | No composite score — raw metrics only |
| `labels.py` with `final_score()` | Ad-hoc 4-factor score (balance/techs/fleet/throughput), not aligned with game dimensions |
| `sim_bench` with Parquet export | No scoring columns, no cross-seed score comparison |
| `AdvisorDigest` with bottleneck detection | No score tracking, no threshold events |
| `autopilot.json` with strategy config | Not a full behavioral config — just content ID mappings |
| Hardcoded autopilot behaviors | No way to compare alternative strategies quantitatively |

**Result:** We can detect bottlenecks but can't answer "Is this run going well?" or "Is config A better than config B across 100 seeds?"

## Proposed Solution

### Scoring Dimensions

Six weighted dimensions computed from existing `GameState` + `MetricsSnapshot` fields:

| Dimension | Weight | Inputs (from existing metrics) | Normalization |
|---|---|---|---|
| **Industrial Output** | 25% | `total_material_kg`, assembler `active` count, per-element production rates | kg processed per tick, normalized to [0, 1] via content-defined ceiling |
| **Research Progress** | 20% | `techs_unlocked` / total available techs, `total_scan_data` growth rate | Fraction complete + rate trend bonus |
| **Economic Health** | 20% | `balance` trend, `export_revenue_total` growth, balance stability (stddev over window) | Revenue per tick + balance floor bonus |
| **Fleet Operations** | 15% | `fleet_total`, `fleet_idle_ratio` (inverted), ships constructed (from counters), mission completion rate | Active fleet utilization + fleet growth |
| **Efficiency** | 10% | `avg_module_wear` (inverted), power utilization ratio, `station_storage_used_pct` (penalize both extremes) | Composite of 3 sub-scores |
| **Expansion** | 10% | Station count, zones with ship activity, fleet size | Count-based with diminishing returns |

**Normalization approach:** Per-tick normalization so runs of different lengths are comparable. Each dimension scores 0.0-1.0, composite = weighted sum × scale factor to reach named thresholds.

**Named thresholds:** Startup (0-200) -> Contractor (200-500) -> Enterprise (500-1000) -> Industrial Giant (1000-2000) -> Space Magnate (2000+)

**Content-driven weights** via `content/scoring.json` — sim_bench scenarios can override weights for focused analysis (e.g., "economy-focused" scoring profile).

### AutopilotConfig Baseline

Formalize the current hardcoded autopilot behavior as an explicit, versioned JSON config. This does NOT change behavior — it makes it measurable and comparable.

Current `content/autopilot.json` has content ID mappings (element names, tech IDs, task priority). The baseline AutopilotConfig extends this with behavioral parameters that are currently hardcoded in `sim_control` concerns:

- Mining element priorities and thresholds
- Lab assignment strategy (which domains to prioritize)
- Export decision criteria
- Ship fitting preferences
- Power shedding priority order

### AI Evaluation Framework

Compare autopilot configs by running the same seeds with different configs and diffing score trajectories:

```
Config A (baseline) × 20 seeds → score distribution A
Config B (variant)  × 20 seeds → score distribution B
→ Per-dimension comparison + composite delta + statistical significance
```

### Optimization Scaffolding

Grid search over 3-5 AutopilotConfig parameters via sim_bench:
1. Generate N config variants (parameter combinations)
2. Run each config across M seeds
3. Collect composite scores
4. Rank by mean score, report variance
5. Identify best config → new baseline candidate

This proves the loop works. Sophistication (Bayesian optimization, scipy.optimize) comes in P6.

## Technical Approach

### Architecture

**Scoring lives in sim_core** as a pure function — same function called by sim_bench, sim_daemon, sim_cli. Deterministic, no IO, no state mutation.

```
sim_core/src/scoring.rs (NEW)
  ├── RunScore struct (per-dimension + composite + threshold name)
  ├── ScoringConfig struct (loaded from content/scoring.json)
  ├── compute_run_score(&GameState, &GameContent, &Constants, &ScoringConfig) -> RunScore
  └── ScoreThreshold enum (Startup, Contractor, Enterprise, IndustrialGiant, SpaceMagnate)

content/scoring.json (NEW)
  ├── dimensions: [{id, name, weight, ceiling, formula_hint}]
  ├── thresholds: [{name, min_score}]
  └── computation_interval_ticks: 24

content/autopilot_baseline.json (NEW)
  ├── extends current autopilot.json
  ├── adds: mining_priorities, lab_strategy, export_thresholds, etc.
  └── version: "baseline-v1"
```

**Key constraint:** `compute_run_score()` only reads existing `GameState` and `MetricsSnapshot` fields. It does NOT add new tick steps or state mutations. This is pure measurement.

### Integration Points

| System | Integration | Files Touched |
|---|---|---|
| sim_core | New `scoring.rs` module, `RunScore` type | `sim_core/src/scoring.rs` (new), `sim_core/src/lib.rs` |
| sim_world | Load `scoring.json` into `GameContent` | `sim_world/src/lib.rs`, `content/scoring.json` (new) |
| sim_bench | Score in summary + Parquet columns | `sim_bench/src/summary.rs`, `sim_bench/src/parquet_writer.rs`, `sim_bench/src/main.rs` |
| sim_daemon | `GET /api/v1/score`, SSE events, advisor digest | `sim_daemon/src/routes.rs`, `sim_daemon/src/analytics.rs` |
| sim_cli | Optional score display at metrics intervals | `sim_cli/src/main.rs` |
| scripts/analysis | `scoring_dimensions()` in labels.py | `scripts/analysis/labels.py`, `scripts/analysis/features.py` |
| ui_web | ScorePanel component | `ui_web/src/components/ScorePanel.tsx` (new) |
| mcp_advisor | Score in metrics digest | `mcp_advisor/src/tools/` |

## Implementation Tickets

### Ticket 1: Scoring content schema and RunScore types

**What:** Define `content/scoring.json` schema and corresponding Rust types. Load scoring config as part of `GameContent` in sim_world.

**Details:**
- `ScoringConfig` struct: dimension definitions (id, name, weight, ceiling), threshold definitions (name, min_score), computation_interval_ticks
- `RunScore` struct: per-dimension scores (`BTreeMap<String, f64>`), composite score (f64), threshold name (String), tick computed at
- `DimensionScore` struct: id, name, raw_value, normalized (0.0-1.0), weighted contribution
- Content loading in `sim_world::load_content()` — add `scoring: ScoringConfig` to `GameContent`
- Schema validation: weights must sum to 1.0, thresholds must be ascending, dimensions non-empty

**Acceptance criteria:**
- [ ] `content/scoring.json` exists with 6 dimensions and 5 thresholds
- [ ] `ScoringConfig` and `RunScore` types compile and serialize/deserialize
- [ ] `load_content()` loads scoring config without error
- [ ] Unit test: valid scoring config loads successfully
- [ ] Unit test: invalid config (weights don't sum to 1.0) rejected
- [ ] Schema documented in `docs/reference.md`

**Dependencies:** None
**Estimated size:** Small

---

### Ticket 2: compute_run_score() pure function

**What:** The core scoring logic in `sim_core/src/scoring.rs`. A pure function that reads `GameState` + `GameContent` + `Constants` + `ScoringConfig` and returns `RunScore`.

**Details:**
- Each dimension has a computation function that reads existing MetricsSnapshot fields and GameState
- **Industrial Output:** `total_material_kg / tick` normalized by ceiling from config. Include assembler active count as throughput signal.
- **Research Progress:** `techs_unlocked / total_techs_available` as base + `total_scan_data` growth rate as trend bonus
- **Economic Health:** Balance trend (short window avg vs long window avg) + `export_revenue_total / tick` + floor bonus if balance > starting_balance
- **Fleet Operations:** `(fleet_total - fleet_idle) / fleet_total` as utilization + ships_constructed from counters + fleet size bonus
- **Efficiency:** Inverted `avg_module_wear` + `power_consumed_kw / power_generated_kw` capped at 1.0 + storage utilization (penalize <10% and >95%)
- **Expansion:** Station count + zones with recent ship activity + fleet size (diminishing returns via sqrt)
- Composite = sum of (dimension_normalized × weight) × scale_factor
- Threshold determined by composite score against sorted threshold list

**Acceptance criteria:**
- [ ] `compute_run_score()` returns deterministic results (same state -> same score)
- [ ] All 6 dimensions produce values in [0.0, 1.0]
- [ ] Composite score scales to named threshold ranges
- [ ] Unit test: known fixture state produces exact expected per-dimension scores
- [ ] Unit test: tick-0 state (minimal activity) scores in Startup range
- [ ] Unit test: advanced state (full activity) scores in Enterprise+ range
- [ ] Unit test: per-tick normalization makes 1000-tick and 5000-tick runs comparable
- [ ] Integration test with `load_content("../../content")` real content values — non-trivial score on dev_advanced_state after 500 ticks
- [ ] No state mutation — function takes immutable references only

**Dependencies:** Ticket 1
**Estimated size:** Medium

---

### Ticket 3: sim_bench scoring integration

**What:** Compute score at metrics intervals during sim_bench runs. Include in summary output, Parquet export, and cross-seed comparison.

**Details:**
- Call `compute_run_score()` alongside `compute_metrics()` at each metrics interval
- **Parquet columns:** `score_composite`, `score_industrial`, `score_research`, `score_economic`, `score_fleet`, `score_efficiency`, `score_expansion`, `score_threshold` (string)
- **Summary output:** Final composite score per seed. Cross-seed stats: mean, min, max, stddev of composite. Per-dimension cross-seed stats.
- **batch_summary.json:** Include score distribution summary
- **Collapse detection extension:** A seed with composite score < threshold at final tick gets flagged

**Acceptance criteria:**
- [ ] Parquet files contain 7 score columns + threshold column
- [ ] `summary.json` includes score stats (mean/min/max/stddev) across seeds
- [ ] `batch_summary.json` includes score distribution
- [ ] Scoring does not measurably impact sim_bench throughput (< 1% overhead measured via TickTimings)
- [ ] Existing `scenarios/ci_smoke.json` produces non-degenerate scores (stddev > 0 across seeds)
- [ ] New Parquet columns documented in `docs/reference.md`

**Dependencies:** Ticket 2
**Estimated size:** Medium

---

### Ticket 4: sim_daemon score endpoint and SSE events

**What:** `GET /api/v1/score` returning current RunScore. Score included in advisor digest. SSE events emitted on threshold crossings.

**Details:**
- **Endpoint:** `GET /api/v1/score` returns `RunScore` JSON (per-dimension breakdown + composite + threshold)
- **Computation:** Score computed every `computation_interval_ticks` (default 24), cached in daemon state alongside metrics history
- **AdvisorDigest extension:** Add `score: Option<RunScore>` field to digest response. Include score trend (improving/declining/stable).
- **SSE event:** New `Event::ScoreThresholdCrossed { previous_threshold, new_threshold, composite_score }` emitted when composite score crosses a named threshold boundary. Add to `applyEvents.ts` handler (per event sync rule).
- **Score history:** Rolling buffer (same size as metrics history) for trend computation

**Acceptance criteria:**
- [ ] `GET /api/v1/score` returns valid RunScore JSON
- [ ] Score updates every 24 ticks in daemon
- [ ] AdvisorDigest includes score and score trend
- [ ] SSE `ScoreThresholdCrossed` event fires on threshold transitions
- [ ] `applyEvents.ts` handles `ScoreThresholdCrossed` event
- [ ] `scripts/ci_event_sync.sh` passes with new event variant

**Dependencies:** Ticket 2
**Estimated size:** Medium

---

### Ticket 5: ML pipeline scoring dimensions

**What:** Extend `scripts/analysis/labels.py` to compute scoring dimensions from Parquet data. Replace the ad-hoc `final_score()` with the official 6-dimension scoring system.

**Details:**
- New function `scoring_dimensions(rel) -> Relation` — computes all 6 dimension scores from existing Parquet columns via DuckDB SQL. Same logic as `compute_run_score()` but operating on columnar data.
- Replace or deprecate existing `final_score()` (currently: 30% balance, 30% techs, 20% fleet, 20% throughput) with the new 6-dimension system. Keep `final_score()` as `legacy_final_score()` for backward compatibility.
- New function `score_trajectory(rel) -> Relation` — score at each metrics interval for trajectory analysis (rising curve vs flat vs declining)
- Cross-seed score comparison: `score_distribution(rel) -> Relation` — per-seed final scores with stats
- Tests with fixture Parquet data

**Acceptance criteria:**
- [ ] `scoring_dimensions()` produces 6 dimension columns + composite matching Rust implementation
- [ ] `score_trajectory()` produces per-tick score curves per seed
- [ ] `score_distribution()` produces cross-seed stats
- [ ] Existing `final_score()` preserved as `legacy_final_score()`
- [ ] `ruff check`, `mypy`, `pytest` all pass
- [ ] Test with real sim_bench Parquet output produces non-trivial scores

**Dependencies:** Ticket 3 (needs Parquet score columns for validation)
**Estimated size:** Medium

---

### Ticket 6: Data gap detection

**What:** Automated analysis script that identifies missing or thin metric coverage relative to scoring dimensions. "Which scoring dimensions have no signal? Which have zero variance across seeds?"

**Details:**
- Python script: `scripts/analysis/data_gaps.py`
- Reads sim_bench Parquet output, computes per-dimension statistics
- Flags: dimensions with zero variance (no discriminating signal), dimensions with null/zero values across all seeds, dimensions where the input metrics are constant (not changing over time)
- Output: structured JSON report + human-readable summary
- Can run standalone or as part of sim_bench post-processing
- **P1 forward-looking:** This script will be critical when new systems (milestones, satellites, stations) add scoring signals. It answers "did we forget to wire up scoring for the new system?"

**Acceptance criteria:**
- [ ] Script runs against sim_bench output directory and produces gap report
- [ ] Detects at least 1 known gap in current sim (Expansion dimension will be thin — only 1 station, no zone diversity)
- [ ] JSON output parseable by other tools
- [ ] `ruff check`, `mypy`, `pytest` all pass
- [ ] CI integration: runs as part of `ci_bench_smoke.sh` (warning, not blocking)

**Dependencies:** Ticket 3 (needs Parquet with score columns)
**Estimated size:** Small

---

### Ticket 7: Baseline AutopilotConfig extraction

**What:** Formalize the current hardcoded autopilot behavior as an explicit, versioned JSON config file. Extract behavioral parameters from sim_control concerns into `content/autopilot_baseline.json`.

**Details:**
- Audit all `sim_control` concern files for hardcoded decision parameters:
  - `ModuleConcern`: module enable/disable thresholds, power shedding priority
  - `LabConcern`: domain prioritization strategy, lab assignment logic
  - `TradeConcern` / `ExportConcern`: export candidate filtering, reserve thresholds (already partially in autopilot.json)
  - `ImportConcern`: import triggers, quantity decisions
  - `ShipFittingConcern`: ship construction triggers
  - `PropellantConcern`: refueling thresholds
  - `ShipAgent`: task priority (already in autopilot.json), mining target selection criteria
- Create `AutopilotConfig` struct in sim_control that loads from `content/autopilot_baseline.json`
- **Key constraint:** Loading the baseline config MUST produce identical behavior to the current hardcoded autopilot. This is extraction, not redesign.
- Extend existing `autopilot.json` or create new file alongside it — design decision during implementation

**Acceptance criteria:**
- [ ] `AutopilotConfig` struct captures all extracted parameters
- [ ] Loading baseline config produces identical sim output to current hardcoded behavior (regression test: same seed + same ticks = same final state)
- [ ] At least 10 behavioral parameters extracted (beyond current autopilot.json content ID mappings)
- [ ] Config file versioned (`"version": "baseline-v1"`)
- [ ] Unit test: default config matches hardcoded behavior across 1000 ticks
- [ ] Integration test: dev_advanced_state + baseline config + seed 42 produces identical MetricsSnapshot at tick 500 vs current code

**Dependencies:** None (parallel with tickets 1-6)
**Estimated size:** Large (requires auditing all concerns)

---

### Ticket 8: AI evaluation framework

**What:** sim_bench infrastructure to compare two AutopilotConfig files by running the same seeds with each and diffing score trajectories.

**Details:**
- New sim_bench mode: `--compare config_a.json config_b.json --seeds 1..20`
- For each seed, runs simulation twice (once per config), collects score at each metrics interval
- Output: `comparison_report.json` with:
  - Per-dimension score deltas (config B - config A) with mean/stddev across seeds
  - Composite score delta with statistical significance (paired t-test or Wilcoxon)
  - Score trajectory divergence points (tick where configs start differing)
  - Per-seed breakdown for debugging
- Human-readable summary: "Config B improves Industrial Output by +12% (p<0.05) but reduces Efficiency by -3% (not significant)"
- **This is the core "is config A better than config B?" tool that P6 optimization will use heavily**

**Acceptance criteria:**
- [ ] `sim_bench compare` subcommand works end-to-end
- [ ] Comparison report includes per-dimension deltas with statistical significance
- [ ] Running baseline vs baseline produces ~0 delta (sanity check)
- [ ] Running baseline vs deliberately-worse config shows negative delta
- [ ] Report includes trajectory divergence analysis
- [ ] JSON output parseable by optimization scripts

**Dependencies:** Ticket 3 (scoring in sim_bench), Ticket 7 (baseline config to compare against)
**Estimated size:** Medium-Large

---

### Ticket 9: Early optimization scaffolding

**What:** Python script that runs grid search over AutopilotConfig parameters via sim_bench, collects scores, ranks configs. Proves the measurement-to-optimization loop works end-to-end.

**Details:**
- Script: `scripts/analysis/optimize_config.py`
- Takes: baseline config path, parameter grid definition (JSON), seeds count, ticks count
- Parameter grid example: `{"mining_priorities.Fe_weight": [0.5, 0.7, 0.9], "lab_strategy.domain_priority": ["balanced", "industrial_first"]}`
- For each parameter combination: generates config variant, runs sim_bench, collects composite score
- Output: ranked configs by mean composite score, per-dimension breakdown, variance analysis
- **Intentionally simple** — grid search over 3-5 parameters is enough. Bayesian optimization comes in P6.
- Uses sim_bench compare infrastructure (Ticket 8) for statistical rigor

**Acceptance criteria:**
- [ ] Script generates N config variants from parameter grid
- [ ] Each variant runs across M seeds via sim_bench
- [ ] Output ranks configs by composite score with confidence intervals
- [ ] At least one non-baseline config ranks differently from baseline (proves the loop discriminates)
- [ ] `ruff check`, `mypy`, `pytest` all pass
- [ ] End-to-end test with 2 parameters × 3 values × 5 seeds completes in reasonable time

**Dependencies:** Ticket 7 (baseline config), Ticket 8 (comparison framework)
**Estimated size:** Medium

---

### Ticket 10: UI score panel

**What:** React component displaying per-dimension score breakdown, composite rating with named threshold, and trend sparklines. Reads from daemon score API.

**Details:**
- New component: `ScorePanel.tsx` in `ui_web/src/components/`
- Fetches from `GET /api/v1/score` (polled or derived from SSE events)
- Display:
  - Composite score + named threshold (e.g., "Enterprise (847)")
  - Per-dimension bar chart or radar chart showing 6 dimensions
  - Trend sparklines (last 100 score computations) showing score trajectory
  - Threshold progress indicator (how far to next threshold)
- Styling: follows existing panel patterns (draggable, Tailwind v4)
- Colors from `config/theme.ts` — dimension colors must be content-scalable (no hardcoded hex per dimension name)
- Wrapped in ErrorBoundary per review checklist
- Responsive: min-w floor on flex items, works at ~200px and full width

**Acceptance criteria:**
- [ ] ScorePanel renders all 6 dimensions with correct values
- [ ] Composite score and threshold name displayed prominently
- [ ] Trend sparklines show score history
- [ ] Threshold crossing triggers visual feedback (highlight, animation)
- [ ] ErrorBoundary wraps the panel
- [ ] Dimension colors sourced from `config/theme.ts`
- [ ] vitest coverage for rendering, threshold transitions, empty state
- [ ] Responsive at 200px and full width

**Dependencies:** Ticket 4 (daemon score endpoint)
**Estimated size:** Medium

---

### Ticket 11: Scoring calibration scenarios

**What:** sim_bench scenarios specifically designed to validate scoring produces meaningful, non-degenerate distributions. These serve as regression tests for the scoring system itself.

**Details:**
- `scenarios/scoring_calibration.json` — runs dev_advanced_state for 2000 ticks across 20 seeds. Validates: composite score reaches Enterprise tier, all 6 dimensions have non-zero values, cross-seed stddev < 20% of mean (not too random).
- `scenarios/scoring_baseline.json` — establishes the canonical baseline score distribution. Run with baseline AutopilotConfig. Results saved as reference for future comparison.
- `scenarios/scoring_dimensions.json` — tests that disabling specific systems (e.g., no mining, no research) produces expected dimension drops. Uses constant overrides to create controlled conditions.
- Update `ci_bench_smoke.sh` to include a quick scoring validation (5 seeds, 500 ticks, check non-degenerate)

**Acceptance criteria:**
- [ ] `scoring_calibration.json` passes (Enterprise tier reached, all dimensions non-zero, reasonable variance)
- [ ] `scoring_baseline.json` produces reproducible reference distribution
- [ ] `scoring_dimensions.json` validates dimension independence (disabling mining tanks Industrial but not Research)
- [ ] CI smoke includes scoring sanity check
- [ ] Scenario results documented as scoring system baseline

**Dependencies:** Ticket 3 (scoring in sim_bench), Ticket 7 (baseline config)
**Estimated size:** Small-Medium

---

## Dependency Graph

```
Ticket 1: Scoring content schema + types ──────────────────────────┐
    │                                                               │
    Ticket 2: compute_run_score() ─────────────────────────────┐   │
    │          │            │                                    │   │
    │     Ticket 3:    Ticket 4:                                │   │
    │     sim_bench    sim_daemon                               │   │
    │     scoring      score API                                │   │
    │       │  │          │                                     │   │
    │       │  │     Ticket 10:                                 │   │
    │       │  │     UI score panel                             │   │
    │       │  │                                                │   │
    │    Ticket 5:   Ticket 6:                                  │   │
    │    ML pipeline  Data gap                                  │   │
    │    scoring      detection                                 │   │
    │                                                           │   │
    Ticket 7: Baseline AutopilotConfig ─────────── (parallel) ──┘   │
    │          │                                                    │
    │     Ticket 8: AI evaluation framework                         │
    │          │                                                    │
    │     Ticket 9: Optimization scaffolding                        │
    │                                                               │
    Ticket 11: Calibration scenarios ── (needs 3 + 7) ─────────────┘
```

**Critical path:** 1 → 2 → 3 → 8 → 9 (scoring types → scoring function → bench integration → evaluation → optimization)

**Parallelizable streams:**
- Stream A (scoring): 1 → 2 → 3 → 5 → 6
- Stream B (AI config): 7 (can start immediately, parallel with Stream A)
- Stream C (API + UI): 2 → 4 → 10
- Convergence: 8 needs both Stream A (ticket 3) and Stream B (ticket 7)
- Ticket 11 needs tickets 3 + 7

**Recommended execution order (single-threaded):**
1. Ticket 1 (schema + types)
2. Ticket 2 (scoring function)
3. Ticket 7 (baseline config — can interleave with 2)
4. Ticket 3 (sim_bench integration)
5. Ticket 4 (daemon endpoint + SSE)
6. Ticket 5 (ML pipeline)
7. Ticket 6 (data gap detection)
8. Ticket 8 (evaluation framework)
9. Ticket 9 (optimization scaffolding)
10. Ticket 10 (UI score panel)
11. Ticket 11 (calibration scenarios)

## System-Wide Impact

### Interaction Graph

- `compute_run_score()` reads `GameState` + `MetricsSnapshot` — pure read, no callbacks or side effects
- `ScoreThresholdCrossed` event flows through existing event pipeline: sim_core emits → sim_daemon broadcasts via SSE → ui_web `applyEvents.ts` handles
- AdvisorDigest gains `score` field — MCP advisor `get_metrics_digest` returns it to balance analysis sessions
- sim_bench summary gains score stats — affects `batch_summary.json` schema (downstream consumers must handle new fields)

### Error Propagation

- Scoring config load failure: `load_content()` returns error → sim_world rejects startup (fail-fast, same as other content errors)
- Score computation with missing metrics (e.g., no ships yet): dimension returns 0.0, not error. All dimensions handle zero/empty state gracefully.
- Parquet schema change: new columns are additive — existing analysis scripts ignore unknown columns (DuckDB's default)

### State Lifecycle Risks

- **None.** Scoring is pure computation — no state persistence, no mutations, no side effects. Score is recomputed from current state on demand. No risk of orphaned or stale scoring state.
- sim_daemon caches computed score for API responses — cache invalidated every `computation_interval_ticks` ticks. Stale cache = slightly old score (not incorrect).

### API Surface Parity

- `GET /api/v1/score` — new endpoint, no parity concerns
- `GET /api/v1/advisor/digest` — extended with `score` field (additive, backward compatible)
- SSE stream — new `ScoreThresholdCrossed` event type (additive)
- MCP advisor — `get_metrics_digest` tool returns score data (additive)
- sim_bench CLI — new `--compare` subcommand and score columns in output (additive)

### Integration Test Scenarios

1. **End-to-end scoring loop:** Start daemon → run 100 ticks → GET /api/v1/score → verify non-zero composite
2. **Threshold crossing SSE:** Run daemon → speed up → listen for ScoreThresholdCrossed event → verify threshold name
3. **sim_bench parity:** Run sim_bench + run daemon with same seed → compare final scores → must match (determinism)
4. **Config comparison:** Run sim_bench compare with baseline vs baseline → delta must be ~0
5. **ML pipeline consistency:** Run sim_bench → export Parquet → compute scoring in Python → compare with Rust scores → must match within floating-point tolerance

## P1 Interface Contract

P0 establishes infrastructure that P1 (Starting State & Progression Engine) will extend:

| P0 Delivers | P1 Will Use It For |
|---|---|
| `RunScore` type | Measuring progression starting state quality vs dev_advanced_state |
| `compute_run_score()` | Score curve comparison: rising curve (progression) vs flat curve (advanced) |
| `content/scoring.json` weights | Potentially different weights for progression vs sandbox scoring profiles |
| `AutopilotConfig` baseline | Reference point for progression-aware config variants |
| sim_bench compare mode | Quantifying "is progression_start better than dev_advanced_state?" |
| Data gap detection | Identifying missing signals after adding milestones/grants |
| Optimization scaffolding | Tuning milestone grant amounts, starting balance, pacing |
| UI score panel | Displaying progression score to validate player experience |

**P1 will add to scoring:**
- Milestone completion timing as a scoring signal (or new dimension)
- Phase advancement rate
- ProgressionState fields readable by `compute_run_score()`

**P6 will mature the loop:**
- Full Bayesian optimization replacing grid search
- Trained scoring models (XGBoost) exported to Rust for microsecond-speed tactical decisions
- Phase-aware AutopilotConfig with ~20 parameters
- CI regression tests gating AI quality

## Acceptance Criteria

### Functional Requirements

- [ ] 6-dimension scoring computed deterministically from existing game state
- [ ] Scores exported in sim_bench Parquet with cross-seed comparison
- [ ] `GET /api/v1/score` endpoint returns current score
- [ ] SSE events fire on threshold crossings
- [ ] UI panel displays per-dimension scores, composite, trends
- [ ] Baseline AutopilotConfig produces identical behavior to current hardcoded autopilot
- [ ] Config comparison framework can rank two configs by score
- [ ] Grid search optimization finds non-trivial config differences
- [ ] Data gap detection identifies thin dimensions

### Non-Functional Requirements

- [ ] Scoring computation < 1% overhead on sim_bench throughput
- [ ] Score API response < 10ms
- [ ] All scoring code deterministic (same inputs → same outputs)

### Quality Gates

- [ ] sim_core scoring: unit tests with known-state fixtures producing exact expected scores
- [ ] sim_bench: at least one integration test with `load_content("../../content")` real values
- [ ] UI: vitest coverage for ScorePanel rendering and threshold transitions
- [ ] Python: `ruff check`, `mypy`, `pytest` all pass for new analysis scripts
- [ ] Calibration scenarios produce non-degenerate score distributions across 20+ seeds
- [ ] Baseline AutopilotConfig regression test passes (identical behavior)
- [ ] Event sync: `ci_event_sync.sh` passes with new ScoreThresholdCrossed variant

## Standards

- All scoring code has unit tests with known-state fixtures producing exact expected scores
- Scoring function is deterministic (same state -> same score, always)
- sim_bench scenarios validate non-degenerate score distributions across 20+ seeds
- Score Parquet columns documented in `docs/reference.md`
- UI score panel has vitest coverage for rendering and threshold transitions
- AutopilotConfig baseline produces identical behavior to current hardcoded autopilot (regression test)
- Data gap detection script runs as part of `ci_bench_smoke.sh`
- Per CLAUDE.md event sync rule: `ScoreThresholdCrossed` event must have handler in `applyEvents.ts`

## Risk Analysis

### High Risk

**Scoring dimensions are poorly calibrated** — dimensions produce degenerate distributions (all zeros, all maxed, no variance).
- *Likelihood:* Medium (we know Expansion dimension will be thin with 1 station)
- *Impact:* High (meaningless scores undermine every downstream use)
- *Mitigation:* Data gap detection (Ticket 6) catches this. Calibration scenarios (Ticket 11) validate. Start with simple formulas, tune via sim_bench observation.

### Medium Risk

**AutopilotConfig extraction changes behavior** — formalizing hardcoded values accidentally introduces rounding or ordering differences.
- *Likelihood:* Low-Medium
- *Impact:* High (breaks regression test)
- *Mitigation:* Strict regression test: same seed, same ticks, same final state. Extract values literally (copy the hardcoded number, don't "round" it).

**Optimization scaffolding finds nothing interesting** — grid search over 3-5 parameters produces no meaningful score differences.
- *Likelihood:* Medium (current autopilot may already be near-optimal for current sim complexity)
- *Impact:* Low (the loop is proven even if the current sim doesn't have enough decision space)
- *Mitigation:* Include at least one "deliberately bad" config to verify the loop discriminates. The real optimization value comes after P1-P3 add decision complexity.

### Low Risk

**Parquet schema change breaks downstream** — adding score columns to Parquet output breaks existing analysis scripts.
- *Likelihood:* Very Low (DuckDB ignores unknown columns by default)
- *Mitigation:* Additive columns only. Existing scripts unaffected.

## Sources & References

### Origin

- **Origin document:** [docs/plans/2026-03-28-003-feat-game-progression-system-plan.md](../../docs/plans/2026-03-28-003-feat-game-progression-system-plan.md) — Project 0 section (lines 114-160). Key decisions carried forward: scoring as pure function in sim_core, per-tick normalization, content-driven weights, baseline config extraction only.

### Internal References

- `crates/sim_core/src/metrics.rs` — MetricsSnapshot (50+ fields feeding scoring dimensions)
- `crates/sim_bench/src/summary.rs` — Cross-seed aggregation (extend with scoring)
- `crates/sim_bench/src/parquet_writer.rs` — Parquet export (add score columns)
- `crates/sim_daemon/src/routes.rs` — API endpoints (add /api/v1/score)
- `crates/sim_daemon/src/analytics.rs` — AdvisorDigest, bottleneck detection, trend tracking
- `crates/sim_control/src/lib.rs` — AutopilotController (extract config from)
- `crates/sim_control/src/agents/` — Concern files (hardcoded params to extract)
- `content/autopilot.json` — Current strategy config (content ID mappings)
- `scripts/analysis/labels.py` — Existing `final_score()` (to be replaced/aligned)
- `scripts/analysis/features.py` — Feature engineering (throughput, utilization)
- `ui_web/src/components/` — Existing panel patterns (FleetPanel, EconomyPanel)
- `docs/DESIGN_SPINE.md` — Design philosophy
- `docs/BALANCE.md` — Current balance observations

### External References

- Civilization VI scoring — multi-dimensional score (cities + citizens + techs + districts + wonders)
- SimCity approval rating — category breakdowns with composite rating
- Factorio production stats — implicit scoring via production/consumption rates
- Stellaris score — fleet power + tech + economy + territory, visible in-game

### Related Work

- VIO-502: Plan P0: Scoring & Measurement Foundation (this ticket's parent)
- VIO-402: Metrics derive macro (nice-to-have prerequisite — reduces boilerplate for new metric fields)
- Linear comment on VIO-502: External design review recommending initial (not final) AutopilotConfig, iterative scoring extension per project
