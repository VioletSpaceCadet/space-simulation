---
date: 2026-03-21
topic: ai-knowledge-ml-pipeline
---

# AI Knowledge System & ML Pipeline

## Problem Frame

The autopilot makes decisions via hardcoded heuristics (mine highest-Fe asteroid, staff modules by priority, build ships when idle). As the game adds hull classes, crew, manufacturing DAGs, and events, the decision space becomes too complex for hand-tuned rules. The game needs a layered AI system: fast deterministic rules for operational decisions, learned models for tactical scoring, and LLM reasoning for strategic/creative decisions. The foundation is a data pipeline that turns sim_bench runs into training data.

## Requirements

### Sim Data Export Pipeline

- R1. **Parquet export from sim_bench.** After each batch run, export per-tick game state snapshots to Parquet files. Columns: tick, per-station metrics (inventory levels, module states, crew counts, power/thermal), per-ship metrics (task, cargo, location), economy (balance, imports, exports), research state. Leverages existing MetricsSnapshot data.
- R2. **DuckDB analysis scripts.** Python scripts that load Parquet files and compute derived features: bottleneck detection, throughput trends, fleet utilization patterns, resource flow rates. Output: feature tables suitable for model training.
- R3. **Outcome labeling.** Automated labeling of sim runs: did this run collapse? When did storage saturate? When did research stall? What was the final economy score? Labels derived from existing sim_bench summary metrics and run_result collapse detection.

### First ML Models (Supervised Learning)

- R4. **Bottleneck prediction model.** Given state at tick N, predict whether storage saturation, crew starvation, or economic collapse occurs within 2000 ticks. Binary classification. Train on sim_bench bulk runs. Replaces hardcoded alert thresholds with a learned predictor.
- R5. **Asteroid scoring model.** Given asteroid features (composition, distance, anomaly tags) + station state (inventory levels, module types, crew), predict mining value. Gradient boosted tree (XGBoost/LightGBM). Replaces hardcoded `fe_mining_value()` with a learned scoring function.
- R6. **Model weights exportable to Rust.** Trained models produce a weight file that can be loaded by the Rust autopilot for inference. Decision trees and linear models are trivial to evaluate in Rust. Neural nets use a minimal inference runtime (burn, tract, or hand-rolled for small nets).

### Knowledge Capture (updates to existing Phase 1)

- R7. **Run journal captures ML-relevant metadata.** Extend journal schema to include: final sim score, collapse tick (if any), bottleneck timeline, autopilot config used. This makes journals double as ML training metadata.
- R8. **Playbook entries link to supporting data.** When a strategy pattern is documented, reference the sim_bench runs that validated it (seed, tick range, Parquet file path). Makes playbook entries verifiable and reproducible.

### LLM Strategic Advisor (evolution of existing MCP advisor)

- R9. **Periodic strategy evaluation.** MCP advisor runs every N ticks (configurable) during daemon operation. Reads current game state via existing tools, consults knowledge base (journals + playbook), outputs strategic recommendations as autopilot config updates (priority weights, template rankings, crew allocation targets).
- R10. **Strategy output is autopilot config, not direct commands.** The LLM writes to a strategy config that the deterministic autopilot consumes. This preserves determinism — same config + same seed = same outcome. The LLM influences behavior through configuration, not tick-level control.
- R11. **Template design via LLM.** When the strategic advisor identifies a fleet gap (need more transport capacity, need volatile mining capability), it can propose a new ship/station template. Template validated by the template system, added to the autopilot's build queue.

## Success Criteria

- sim_bench runs produce Parquet files loadable by DuckDB
- A trained bottleneck predictor outperforms hardcoded alert thresholds on held-out sim runs
- A trained asteroid scorer outperforms hardcoded fe_mining_value() on fleet throughput metrics
- MCP advisor can update autopilot priorities based on game state analysis
- All ML inference runs fast enough for real-time use (<1ms per evaluation)

## Scope Boundaries

- **Not in scope:** Reinforcement learning / policy networks (future — requires entity depth + crew to have interesting action space)
- **Not in scope:** Neural network training infrastructure (start with gradient boosted trees)
- **Not in scope:** Multi-agent RL for NPC civilizations (long-term vision)
- **Not in scope:** Real-time LLM inference in the tick loop (LLM is async, out-of-process only)
- **Design for future:** Parquet schema should be extensible as new systems add metrics. Model interface should support hot-swapping (new model weights loaded without restart).

## Key Decisions

- **Three-layer architecture:** Rules (every tick, Rust) → ML models (tactical scoring, Rust inference) → LLM (strategic, async, out-of-process). Each layer sets goals/constraints for the layer below.
- **Offline training, online inference:** Models trained on sim_bench bulk runs, not during gameplay. Inference is a simple scoring function in Rust. No GPU needed at runtime.
- **LLM as coach, not player:** LLM never touches the tick loop. It writes autopilot configuration. Determinism preserved.
- **Start with supervised learning:** Bottleneck prediction and scoring functions before RL. Validate the data pipeline works before investing in complex training.

## Phasing

### Phase 1: Data Pipeline + Knowledge Capture (do now)
- Parquet export from sim_bench (R1)
- DuckDB analysis scripts (R2)
- Outcome labeling (R3)
- Extend journal schema with ML metadata (R7)
- Existing Phase 1 knowledge tickets (VIO-173 through VIO-178)

### Phase 2: First Models
- Bottleneck prediction model (R4)
- Asteroid scoring model (R5)
- Rust inference integration (R6)
- Playbook data links (R8)

### Phase 3: LLM Strategic Advisor
- Periodic strategy evaluation (R9)
- Strategy-as-config output (R10)
- Template design via LLM (R11)

### Future: RL + Multi-Agent
- Frame autopilot as RL problem (state/action/reward)
- Policy network trained on millions of sim_bench runs
- Multiple MCTS systems for different tactical subproblems
- NPC civilization agents

## Dependencies / Assumptions

- **sim_bench** already produces CSV metrics — Parquet export extends this
- **MCP advisor** already reads game state — strategic advisor extends this
- **StatModifier system** (VIO-332) — autopilot config adjustments flow through it
- **Entity depth + crew** increase action space enough to make ML worthwhile
- **Python + DuckDB + scikit-learn/XGBoost** available locally for training

## Outstanding Questions

### Deferred to Planning
- [Affects R1][Technical] Parquet schema design — which columns from MetricsSnapshot, how often to sample (every tick vs every N ticks)?
- [Affects R6][Needs research] Simplest Rust inference approach for decision trees — hand-rolled evaluator vs burn/tract crate?
- [Affects R9][Technical] How does periodic LLM advisor integrate with daemon — separate process polling API, or MCP tool invoked on timer?
- [Affects R5][Needs research] Feature engineering for asteroid scoring — what features beyond composition and distance matter?

## Next Steps

→ `/ce:plan` for Phase 1 (data pipeline + knowledge capture).
