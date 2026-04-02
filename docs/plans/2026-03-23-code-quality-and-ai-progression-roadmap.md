# Code Quality & AI Progression Roadmap

**Date:** 2026-03-23
**Status:** Draft — detailed planning session TBD
**Scope:** Comprehensive code quality audit findings, prioritized refactoring tickets, and analysis of progression toward AI-driven gameplay.

---

## Part 1: Codebase State Assessment

### By the Numbers

| Metric | Value |
|--------|-------|
| Total Rust LoC | 39,375 |
| Production code | ~22,000 |
| Test code | ~17,000 |
| Test functions | 401 |
| Coverage threshold | 83% line |
| Crates | 6 (sim_core, sim_control, sim_cli, sim_daemon, sim_bench, sim_world) |
| Content JSON files | ~10 |
| Python analysis scripts | 12 |
| ML scenarios | 4 (baseline, constrained, abundant, smoke) |

### Completed Projects (velocity context)

All completed since late February 2026:

- Spatial positioning (hierarchical polar coordinates)
- Energy system (solar arrays, batteries, power stalling)
- Asteroid resource typing (volatile-rich, iron-rich, templates)
- Water-to-propellant chain (electrolysis, boiloff, cryogenics)
- Heat system (thermal groups, radiators, overheat zones)
- Manufacturing DAG (multi-input/output recipes, tech-gated, priority)
- Economy & trade (import/export, pricing, ship construction)
- Research system redesign (lab-based, domain-specific, probabilistic unlock)
- Sim events engine (content-driven, composable effects, deterministic)
- Modifier system (4-phase pipeline: flat, pct additive, pct multiplicative, override)
- MCP balance advisor (metrics digest, bottleneck detection, parameter proposals)
- Knowledge system Phase 1 (run journals, playbook, MCP query/save/update)
- ML data pipeline (Parquet export, DuckDB features/labels, cross-seed analysis, model stubs)
- Solar system map redesign (canvas + DOM overlay)
- BE/FE sync audit

### In Progress / Planned

- **Ship Hull+Slot System** — in progress
- **Crew System** — planned
- **Molten Materials** — backlog
- **Propellant-Based Movement** (Epic 4) — backlog
- **Research Expansion** (Epic 5) — backlog
- **Performance Optimization** — backlog

---

## Part 2: Code Quality Audit Findings

Audit methodology: Linear Code Quality project (59 existing tickets), `docs/solutions/`, `.claude/skills/`, CLAUDE.md review checklist, full codebase grep + rust-analyzer LSP analysis across all crates.

### 2.1 Rust-Specific Quality Issues

#### Floats in Simulation State (Determinism Risk)

`types.rs` has ~120 `f32`/`f64` field declarations on game state types used in tick arithmetic. The documented rule says "integer types for state, floats only at content loading boundary" but actual types contradict this.

| Area | Fields | Risk |
|------|--------|------|
| Inventory mass/quality | `kg: f32`, `quality: f32` on InventoryItem | Used in tick arithmetic |
| Wear state | `wear: f32` on WearState | Compared, accumulated every tick |
| Composition | `CompositionVec = HashMap<ElementId, f32>` | Iterated during processing |
| Research | `difficulty: f32`, `data_generation_peak: f32` | RNG boundary values |
| Ship/station stats | `cargo_capacity_m3: f32`, `propellant_kg: f32` | Task duration calcs |
| Constants | Nearly all tuning params are `f32` | Flow into every system |
| Sim events | `extract_condition_value()` casts tick, len() to f64 | Condition evaluation |
| Boiloff | `boiloff_rate_per_day: f64`, cast back to f32 | Cross-platform divergence risk |

Cross-platform determinism is at risk — f32 arithmetic can differ across x86/ARM/WASM. The thermal system already demonstrates the correct milli-unit integer pattern.

#### Unsafe `as` Casts (7 production sites)

7 float-to-int casts lack `.clamp()` guard — potentially undefined behavior on out-of-range values in release builds:

- `thermal.rs:195` — `delta_mk as u32` (debug_assert only)
- `processor.rs:419` — `delta_mk as u32` (same pattern)
- `composition.rs:85,89` — `.round() as u32/i64` (temp/heat blending)
- `commands.rs:577` — modifier `.resolve() as u64` (ship speed, no clamp)
- `tasks.rs:152,154` — `.ceil() as u64` (tick calculations)

Correct pattern already exists in `thermal.rs:25,45`.

#### Clippy Suppressions (7 production `too_many_lines`)

| File | Function | Lines |
|------|----------|-------|
| `engine.rs:72` | `apply_commands` dispatcher | Thin dispatcher (arguably OK) |
| `processor.rs:189` | `resolve_processor_run` | ~242 lines — needs decomposition |
| `assembler.rs:64,326` | Two assembler functions | ~122 and ~100+ lines |
| `station/mod.rs:188` | `compute_power_budget` | Large coordinator |
| `tasks.rs:440` | Task function | ~100+ lines |
| `sim_events.rs:793` | (too_many_arguments) | Complex signature |

Plus 3 in sim_bench. Project policy: never suppress, decompose instead.

#### f64::EPSILON Bug

`sim_events.rs:91` uses `(lhs - rhs).abs() < f64::EPSILON` for equality comparison. `f64::EPSILON` (~2.2e-16) means computed values will almost never compare equal. Likely broken equality check.

### 2.2 Duplication & Extensibility Bottlenecks

#### The "Add a Metric" Tax — 12 Locations

Every `MetricsSnapshot` field is manually listed in 12 places across 4 files:

1. `metrics.rs` — `MetricsSnapshot` struct field
2. `metrics.rs` — `MetricsAccumulator` struct field
3. `metrics.rs` — `accumulate_station()`/`accumulate_ship()` computation
4. `metrics.rs` — `finalize()` struct literal (~40 fields)
5. `metrics.rs` — `write_metrics_header()` CSV header string
6. `metrics.rs` — `append_metrics_row()` CSV row (**35 positional `{}` placeholders** — silent corruption risk)
7. `parquet_writer.rs` — `RowBuffer` struct field
8. `parquet_writer.rs` — `RowBuffer::new()` initializer
9. `parquet_writer.rs` — `build_schema()` Arrow Field
10. `parquet_writer.rs` — `append_to_buffer()` row append
11. `parquet_writer.rs` — `build_record_batch()` column finish
12. `summary.rs` — `compute_summary()` extractor closures

Adding one new metric = 12 edits across 4 files, ~60 lines of boilerplate. Adding metrics for a new game system (crew, hull damage) with ~5 metrics = 300+ lines of pure sync boilerplate. The Python side (DuckDB) adapts automatically via schema inference — the Rust side is the bottleneck.

#### The "Add a Module Type" Tax — 12 Files, ~150 Lines

Adding a new `ModuleBehaviorDef` variant requires:

| File | What you add |
|------|-------------|
| `types.rs` — `ModuleBehaviorDef` enum | New variant with fields |
| `types.rs` — `ModuleKindState` enum | New runtime state variant |
| `types.rs` — `BehaviorType` enum | New tag |
| `commands.rs:55-113` — `default_module_state()` | Match arm mapping Def→State→BehaviorType |
| `station/mod.rs:86-103` — `tick_stations()` | New tick function call |
| New `station/<module>.rs` file | Actual tick logic |
| `metrics.rs:305-368` — `accumulate_module()` | Match arm for new metrics |
| `overrides.rs:86-130` — `apply_module_override()` | Match arm for bench overrides |
| `sim_world/lib.rs` — `build_initial_state()` | Starting loadout entry |
| `content/module_defs.json` | JSON definition |
| `applyEvents.ts:22` — `MODULE_KIND_STATE_MAP` | TS state mapping |
| `content/dev_advanced_state.json` | Starting state entry |

#### The "Add an Inventory Type" Tax — 136 Match Sites

`InventoryItem` (5 variants) is pattern-matched 136 times across 11 production files. Adding a 6th variant requires updating all sites.

| File | Match sites |
|------|------------|
| `station/processor.rs` | 31 |
| `station/assembler.rs` | 26 |
| `trade.rs` | 17 |
| `metrics.rs` | 15 |
| `commands.rs` | 11 |
| `composition.rs` | 9 |
| `station/boiloff.rs` | 11 |
| Others | 16 |

#### Other Duplication

- **`trade.rs`** — 7 functions × 3 `TradeItemSpec` match arms = 21 arms doing slight variations of the same thing
- **`overrides.rs`** — ~200-line match manually mapping string keys to `Constants` struct fields (struct already derives Deserialize)
- **`behaviors.rs`** — 30+ hardcoded content ID strings (`"module_electrolysis_unit"`, `"Fe"`, `"repair_kit"`, etc.)
- **Hardcoded constants** — 6 Rust `const` values not in `constants.json` (not scenario-tunable)

### 2.3 Test Duplication

~17,000 lines of test code across 401 tests with massive copy-paste:

| Pattern | Instances | Lines wasted |
|---------|-----------|-------------|
| `ModuleState` construction (10 lines each) | 43 copies | ~430 |
| `ModuleDef` construction (12-15 lines each) | 60 copies across 22 files | ~600 |
| `ElementDef` construction (5 of 9 fields always None) | 17 copies | ~170 |
| `InventoryItem::Ore` construction | 27 copies in 9 files | ~200 |
| `StationId("station_earth_orbit".to_string())` | 157 occurrences | ~100 noise |
| Per-file `*_content()` factories | 11 near-identical functions | ~500 |
| `deposit.rs` first 3 tests (90% identical) | 3 tests | ~60 |
| `cold_refinery_regression.rs` (duplicates existing coverage) | 483 lines | ~300 |

Key indicators: `manufacturing_priority: 0` appears 82 times, `wear: WearState::default()` 53 times — always in the same boilerplate block.

### 2.4 File Size Hotspots

| File | Lines | Content |
|------|-------|---------|
| `types.rs` | 1,970 | All types — needs splitting into submodules |
| `sim_events.rs` | 1,971 | Event engine + ~1,000 lines of tests |
| `station/processor.rs` | 1,659 | Processor logic + ~1,000 lines of tests |
| `station/thermal.rs` | 1,503 | Thermal system + ~1,009 lines of tests |
| `station/mod.rs` | 1,187 | Station coordinator + tests |
| `station/assembler.rs` | 1,088 | Assembler + tests |
| `behaviors.rs` | 945 | 9 autopilot behaviors |
| `parquet_writer.rs` | 684 | Hand-rolled ORM for metrics→Arrow |

---

## Part 3: Prioritized Refactoring Tickets

18 tickets created in the Code Quality project (VIO-402 through VIO-419).

### Tier 1 — High Priority (Extensibility blockers, active maintenance pain)

| Ticket | Title | Key Benefit |
|--------|-------|-------------|
| **VIO-402** | Metrics pipeline derive macro | Eliminates 12x-per-field duplication, removes silent CSV corruption risk. ~500 lines saved. |
| **VIO-403** | Test fixture builders | ~1,750 lines of copy-paste reduced. New tests become 5-10 lines instead of 30-50. |
| **VIO-404** | InventoryItem methods | Collapses ~50% of 136 match sites. Adding a 6th variant becomes tractable. |
| **VIO-405** | TradeItemSpec methods | Collapses 7 functions × 3 arms. ~150 lines saved. |
| **VIO-406** | Constants override via serde | Eliminates 200-line match. New constants auto-overridable. |

### Tier 2 — Medium Priority (Code quality, maintainability)

| Ticket | Title | Key Benefit |
|--------|-------|-------------|
| **VIO-407** | ModuleBehaviorDef factory | Eliminates 60-line triple-enum sync match. |
| **VIO-408** | Generic module metrics | New module types auto-get metrics. Blocked by VIO-402. |
| **VIO-409** | Decompose resolve_processor_run | Removes 242-line clippy suppression. |
| **VIO-410** | Remove all clippy suppressions | Policy compliance. Blocked by VIO-409, VIO-406. |
| **VIO-411** | Split types.rs into submodules | 1,970 lines → focused files. |
| **VIO-412** | Content-driven autopilot | Replaces 30+ hardcoded IDs. Key enabler for AI strategy config. |
| **VIO-413** | Float determinism audit | Cross-platform determinism. |
| **VIO-414** | Clamp unsafe casts | 7 one-line fixes. Prevents undefined behavior. |

### Tier 3 — Low Priority (Cleanup)

| Ticket | Title | Key Benefit |
|--------|-------|-------------|
| **VIO-415** | Extract constants to JSON | Scenario-tunable. |
| **VIO-416** | Fix f64::EPSILON comparison | Likely-broken equality check. |
| **VIO-417** | Curate public exports | Clearer API surface. |
| **VIO-418** | Naming improvements | Readability. |
| **VIO-419** | Daemon test unwraps | Better failure messages. |

### Dependency Graph

```
VIO-402 (Metrics derive) ──blocks──> VIO-408 (Generic module metrics)
VIO-404 (InventoryItem methods) ──blocks──> VIO-405 (TradeItemSpec methods)
VIO-409 (Decompose processor) ──blocks──> VIO-410 (Remove all suppressions)
VIO-406 (Constants serde) ──blocks──> VIO-410 (Remove all suppressions)
VIO-411 (Split types.rs) ──related──> VIO-417 (Curate exports)
VIO-406 (Constants serde) ──related──> VIO-415 (Extract constants)
```

### Estimated LoC Impact

| Category | Current | After | Delta |
|----------|---------|-------|-------|
| Rust production code | ~22,000 | ~20,200 | -1,800 |
| Rust test code | ~17,000 | ~15,250 | -1,750 |
| **Total Rust** | **39,375** | **~35,450** | **~-3,900 (-10%)** |

### Post-Refactor Extensibility

**Adding a new element/ore:** JSON entry only (0 Rust files after VIO-412)

**Adding a new module type:** ~6 files / 60 lines (down from 12 files / 150 lines after VIO-407 + VIO-408 + VIO-406)

**Adding a new metric:** 1-line struct field (down from 12 locations / 60 lines after VIO-402)

**Adding a major feature (e.g., crew system):** ~800 lines of focused game logic (down from ~2,000 lines of boilerplate-heavy code). The ratio of "interesting code" to "sync boilerplate" flips from ~40/60 to ~80/20.

---

## Part 4: AI Progression Assessment

### Existing AI/ML Infrastructure

#### Completed

| Component | Description |
|-----------|-------------|
| **sim_bench** | Parallel multi-seed scenario runner with Parquet + CSV output |
| **Parquet export** | Columnar metrics with Zstd compression |
| **DuckDB analysis** | `load_run.py` auto-discovers Parquet/CSV, adds metadata columns |
| **Feature extraction** | `features.py` — throughput rates, fleet utilization, power surplus, storage pressure |
| **Outcome labeling** | `labels.py` — collapse detection, storage saturation, research stall, final score, bottleneck timeline |
| **Cross-seed analysis** | `cross_seed.py` — per-seed summary, aggregate stats, variance analysis |
| **Model stubs** | `bottleneck_stub.py` (majority-class), `scoring_stub.py` — prove pipeline works end-to-end |
| **Training data generation** | `generate_training_data.py` — runs 3 ML scenarios, applies features/labels |
| **ML scenarios** | baseline, constrained, abundant, smoke (diverse training conditions) |
| **MCP balance advisor** | Metrics digest, alerts, parameter proposals, sim lifecycle tools |
| **Knowledge Phase 1** | Run journals (structured JSON), strategy playbook (markdown), MCP query/save/update |
| **Autopilot trait** | `AutopilotBehavior` trait with `name()` + `generate()`, 9 behaviors |
| **Modifier system** | 4-phase pipeline (flat, pct additive, pct multiplicative, override), `StatId` enum |

#### Designed but Not Started

| Component | Ticket | Description |
|-----------|--------|-------------|
| **Three-layer AI architecture** | VIO-181 | L1 deterministic rules, L2 LLM strategic advisor, L3 offline ML |
| **Supervised learning → RL pipeline** | VIO-182 | Bottleneck prediction, scoring functions, tree inference in Rust, future RL |
| **Semantic retrieval** | VIO-179/180 | Vector embeddings, RAG-enhanced recommendations |

### Three-Layer Architecture (VIO-181)

**Key architectural insight:** Time-scale separation means each layer can fail independently without bringing down the others. A bad LLM strategic call doesn't crash the sim — L1 keeps running with sane defaults. A poorly trained scoring model doesn't corrupt game state — it just makes suboptimal recommendations that the next training cycle can correct. Fault isolation by time scale.

```
┌─────────────────────────────────────────────────────────┐
│ Layer 3: Offline ML Optimization                        │
│ (sim_bench, between sessions)                           │
│                                                         │
│ Train on Parquet ──> scoring functions, bottleneck       │
│ prediction, policy networks                             │
│ Export weights ──> Rust for microsecond inference        │
└──────────────────────┬──────────────────────────────────┘
                       │ weights / model files
┌──────────────────────▼──────────────────────────────────┐
│ Layer 2: LLM Strategic Advisor                          │
│ (out-of-process, every ~1000 ticks or major state       │
│  change)                                                │
│                                                         │
│ Reads game state via MCP ──> consults knowledge base    │
│ Outputs: autopilot config, template designs, priorities │
│ Strategy output is CONFIG, not commands (determinism)   │
└──────────────────────┬──────────────────────────────────┘
                       │ AutopilotConfig JSON
┌──────────────────────▼──────────────────────────────────┐
│ Layer 1: Deterministic Rules (Rust, every tick)         │
│                                                         │
│ Autopilot behaviors read from config                    │
│ Power priority, crew assignment, module scheduling      │
│ ML scoring functions replace hardcoded heuristics       │
│ Fast, deterministic, no external dependencies           │
└─────────────────────────────────────────────────────────┘
```

### Layer Readiness Assessment

**Layer 1 (Rust autopilot): ~80% ready**
- `AutopilotBehavior` trait exists with 9 behaviors
- Modifier system can express tech-granted bonuses
- **Gap:** Behaviors are hardcoded if-chains, not tunable. Need `AutopilotConfig` struct that L2 can write and L1 reads (priority weights, thresholds, template rankings). VIO-412 (content-driven autopilot) is the key enabler.

**Layer 2 (LLM advisor): ~55% ready**
- MCP tools exist (digest, alerts, parameters, proposals, knowledge query/save)
- Knowledge Phase 1 done (playbook with real strategic knowledge from actual runs)
- **Gap:** Strategy config output format is entirely undesigned. Advisor currently proposes parameter changes; needs to also output autopilot config and template designs. This is the VIO-181 design doc deliverable — and defining what an `AutopilotConfig` looks like is a design problem that could easily take as long as any single feature project. The ~55% estimate reflects that the infrastructure exists but the hard design work hasn't started.
- **Gap:** Template design interface. Once hull+slots lands, LLM needs a way to express "build ship with hull X, fitted modules Y, Z" as a template L1 executes.

**Layer 3 (Offline ML): ~40% ready**
- Parquet pipeline, DuckDB features/labels, cross-seed analysis, model stubs done
- ML scenarios generate diverse training data (3 scenarios × 20+ seeds)
- **Gap:** Real models — need XGBoost/LightGBM beyond majority-class stubs
- **Gap:** Rust inference — decision tree evaluation in Rust doesn't exist yet
- **Gap:** Feedback loop — trained model → export weights → Rust loads → sim_bench validates → repeat

### The Content Depth Problem

Even with perfect AI architecture, if the decision space is "mine iron or mine iron," there's nothing to optimize. Each upcoming project creates genuine decision dimensions:

| Project | Decision Space It Creates |
|---------|--------------------------|
| **Hull+Slot** (in progress) | Ship design: which hull, which fitted modules, speed vs cargo tradeoff |
| **Crew System** (planned) | Labor allocation: which modules get crew, recruitment timing, automation vs manual tradeoff |
| **Molten Materials** (backlog) | Processing chains: crucible → mold → product, temperature management |
| **Research Expansion** (backlog) | Research prioritization: which domain to focus labs on, efficiency vs capability |
| **Propellant Movement** (backlog) | Logistics: refuel timing, delta-v budgeting, route optimization |
| **Multi-station** (not yet planned) | Expansion strategy: when/where to build, inter-station supply chains |

Each project roughly doubles the autopilot's decision space. After hull+slots + crew, there are enough dimensions that cross-seed optimization becomes genuinely interesting.

### Key Insight: Where MCTS Fits (and Doesn't)

Per VIO-182 revision:

- **MCTS for bounded tactical subproblems:** "Which of 8 asteroids should these 3 idle ships mine?" Tree depth ~3-5, branching factor ~8. Fast rollouts using `sim_core` (clone state, run forward 200 ticks, evaluate).
- **NOT for strategic/design decisions:** "Design a ship template" is combinatorial, not a tree search. LLMs handle this (Layer 2).
- **NOT for parameter optimization:** "What's the optimal mining rate threshold?" is hyperparameter tuning. Gradient boosted trees + sim_bench handle this (Layer 3).

---

## Part 5: Recommended Progression Sequence

**Design philosophy:** Build game depth and deterministic AI first. Classical ML (gradient boosted trees, parameter optimization) delivers the most value soonest because the sim_bench + DuckDB pipeline already exists. LLM integration and RL are powerful but premature — they need a richer decision space and more training data to justify the complexity. Push them to later phases and let game content + classical optimization prove out the architecture first.

### Phase 1: Enable the Strategy Interface

**Goal:** The autopilot becomes configurable, so strategy is a tunable parameter — searchable by optimizer, comparable via sim_bench.

1. **Finish hull+slots** (in progress) — creates ship design decision space
2. **Define AutopilotConfig schema** (VIO-181 deliverable, focused on config format)
   - JSON: priority weights, thresholds, template rankings, manufacturing priorities
   - Autopilot behaviors read from config instead of hardcoded constants
   - sim_bench can override config per scenario
3. **VIO-412: Content-driven autopilot** — replace 30+ hardcoded IDs with role queries
4. **Key code quality tickets** — VIO-406 (constants serde), VIO-403 (test fixtures), VIO-404 (InventoryItem methods)

**Outcome:** `sim_bench --scenario baseline.json --config strategy_a.json` vs `--config strategy_b.json` and compare scores across 100 seeds.

### Phase 2: Classical Optimization Loop

**Goal:** Automated parameter search discovers better strategies than hand-tuned defaults.

5. **Parameter sweep via sim_bench** — Python script that generates N strategy configs, runs each across M seeds, ranks by `final_score`
   - Start simple: `scipy.optimize.minimize` or grid search over 5-10 key parameters
   - No ML framework needed — just the existing DuckDB pipeline + optimization wrapper
6. **Replace bottleneck_stub with XGBoost/LightGBM** — real bottleneck prediction from sim_bench Parquet
7. **Scoring functions** — asteroid mining value, template build priority. Gradient boosted trees trained on outcome data.
8. **A/B validation** — automated sim_bench comparison: heuristic vs optimized, report % improvement

**Outcome:** "Optimized strategy scores 23% higher than default across 200 seeds." First data-driven proof that the pipeline works end-to-end.

**Side benefit:** Phase 2 is also a playtesting accelerator. Running 100 seeds with strategy A vs strategy B reveals dynamics about the simulation that no amount of reading the code will show. "Optimized strategy always builds transport haulers before mining barges" — why? That kind of discovery feeds back into game design, not just AI development.

### Phase 3: Expand the Decision Space

**Goal:** Enough game complexity that optimization is meaningfully interesting — more dimensions, more tradeoffs.

9. **Crew system** — labor allocation, automated vs manual modules, recruitment timing
10. **Content depth** — more recipes, modules, tech tiers, production chains
11. **Research expansion** — more techs, more domains, prioritization tradeoffs
12. **VIO-402: Metrics derive macro** — unblocks adding metrics for new systems without 12x boilerplate

**Outcome:** AI makes real tradeoffs: cargo ship vs mining ship, crew the refinery or the lab, research efficiency or capability.

### Phase 4: Rust-Native Inference

**Goal:** Trained models run inside the sim tick loop at microsecond speed, replacing hardcoded heuristics one at a time.

13. **Decision tree evaluation in Rust** — load XGBoost/LightGBM model, evaluate per tick. No GPU, no external runtime.
14. **Replace heuristics incrementally** — swap `fe_mining_value()` with trained scorer. Validate via sim_bench A/B.
15. **MCTS for bounded tactical subproblems** — "which 3 asteroids for these 3 ships?" Clone state, rollout 200 ticks, evaluate. Tree depth ~3-5, branching ~8.

**Outcome:** The autopilot makes empirically-better decisions at tick speed, validated against baselines.

### Phase 5: Strategic Depth

**Goal:** Multi-location logistics and long-term planning create genuine strategic complexity.

16. **Propellant-based movement** — delta-v budgets, route planning
17. **Multi-station** — build stations at different orbital bodies
18. **Inter-station logistics** — supply chain optimization between stations
19. **Molten materials** — advanced processing chains
20. **Performance optimization** — approach 2M TPS target to enable larger training runs

**Outcome:** A multi-station space economy with supply chains, route optimization, and expansion strategy — all driven by the autopilot config + classical optimization loop.

### Phase 6: LLM & RL Integration (Long-term)

**Goal:** Add the higher-intelligence layers once the game is complex enough to need them.

21. **Knowledge Phase 2** — semantic retrieval (VIO-179/180), RAG-enhanced recommendations
22. **LLM strategic advisor** (Layer 2) — consults knowledge base, outputs autopilot config updates and template designs
23. **RL policy training** — frame full autopilot as RL problem, train on millions of sim_bench runs
24. **Full L1/L2/L3 integration** — ML scoring + LLM strategy + deterministic execution

**Why push this to Phase 6?**
- LLMs and RL are most valuable when the decision space is large. Before Phase 5, classical optimization is simpler and sufficient.
- The `AutopilotConfig` interface from Phase 1 is the same interface the LLM will eventually write to — building it now for classical optimization doesn't waste work.
- RL needs millions of training runs. Performance optimization (Phase 5) makes this practical.
- The knowledge system keeps accumulating during Phases 1-5 via MCP tools. By Phase 6 there's a rich corpus for RAG.

**Outcome:** An AI that designs ship templates, plans expansion, and improves across runs — built on top of a game that's already deep enough to be interesting.

### The Grand Vision: Where LLMs Become Essential

Classical optimization (Phases 1-5) works for single-station, ~10-ship operations. It breaks down when the game reaches true grand strategy scale: hundreds to thousands of ships, entire solar systems, multi-system coordination.

At that scale, the decision space explodes combinatorially. No gradient boosted tree can reason about "should I build a refinery station at Ceres or a fuel depot at Europa to support a fleet expansion toward the outer planets?" That requires multi-step strategic reasoning, understanding of supply chain dependencies, and the ability to plan 10,000 ticks ahead with branching contingencies.

This is where the LLM layer becomes not just helpful but an order of magnitude better than classical game AI:

- **Multi-station supply chain design** — "Europa has water, Ceres has iron, the fleet needs both at Mars" is a natural language planning problem
- **Fleet composition and deployment** — "Send fast scouts to survey, mining fleet follows, establish forward base" is a strategy that emerges from reasoning, not parameter tuning
- **Adaptive crisis response** — "Ore supply collapsed because an asteroid depleted; redirect fleet, pause smelters, import emergency supplies" requires understanding causal chains
- **Cross-run learning at the strategic level** — "Last campaign collapsed when I expanded too fast without fuel infrastructure" is a lesson that transfers to novel situations

The classical optimization layers (L1 + L3) handle the tactics. The LLM layer (L2) handles the strategy. The richer the game gets through Phases 1-5, the more valuable the LLM becomes — which is exactly why it should come last.

The difference is between AI that *optimizes* and AI that *strategizes*. Classical game AI can play competently. An LLM-integrated system can play *interestingly* — making plans that have narrative coherence, responding to crises with causal reasoning, learning lessons that transfer to novel situations. The deterministic, fast sim makes this tractable in a way it wouldn't be for almost any other game — that's the genuine competitive advantage.

**Biggest risk:** Sustaining development long enough to reach Phase 6. The mitigation is built into the plan: each phase delivers standalone value. Phase 2's optimization loop is interesting even if Phase 6 never ships. That's the design intent.

---

## Part 6: What Will Be Hard

### The AutopilotConfig Schema Is the Crux

Getting the config format right — expressive enough for meaningful strategy choices, constrained enough for deterministic execution — is a design problem, not an implementation problem. It deserves its own multi-section design session with the same rigor as events, hulls, crew, and manufacturing. It's the interface between human intent, classical optimization, and eventually LLM reasoning. If it's too rigid, the optimizer can't explore. If it's too loose, the search space explodes.

The sweet spot is probably hierarchical: high-level strategy enums (`expand` / `consolidate` / `optimize`) that set broad parameters, with per-system tuning knobs underneath. Design for `scipy.optimize` first — the LLM interface will be a superset later.

### Content Depth Is the Rate Limiter — and Refactors Are AI Infrastructure

Even with perfect AI architecture, if the decision space is "mine iron or mine iron," there's nothing to optimize. The code quality refactors (Part 3) directly impact how fast content can be added. After the refactors, adding 10 new modules is ~600 lines of focused game logic instead of ~1,500 lines of boilerplate. This is what enables Phase 3 (expanding the decision space) to move at the speed the optimization loop needs.

The code quality audit quantifies this precisely: adding a new module type costs 12 files and 150 lines of boilerplate. Every new decision dimension for the AI to optimize over has a high implementation tax. The refactoring tickets (VIO-402 through 407 especially) aren't just code hygiene — they're AI infrastructure. Cutting the "add a module" cost from 150 lines to 60 lines means the decision space can grow in half the time, which means the optimization loop gets interesting sooner.

### ML Training Needs Scale (Eventually)

Current: 3 scenarios x 20 seeds = 60 runs. Sufficient for gradient boosted trees (Phase 2). Not enough for RL (Phase 6). At ~435K TPS, 10,000 ticks x 1,000 seeds = ~23 seconds. At 2M TPS target = ~5 seconds. Classical optimization (Phase 2) works fine at current speed. RL (Phase 6) needs the Performance project.

### MCTS Clone Cost Needs Early Validation

The "clone state, rollout 200 ticks" assumption in Phase 4 (MCTS for tactical subproblems) needs benchmarking before building the infrastructure. How large is `GameState`? If it's a few megabytes with `BTreeMap`s and `Vec`s, cloning it thousands of times for MCTS rollouts has memory implications. Tick throughput is fast, but clone cost could dominate. Worth benchmarking a state clone early to know whether a copy-on-write state representation is needed.

### Float Determinism Affects ML Training Data

The float determinism audit (VIO-413) has implications beyond cross-platform correctness. If ML models are trained on `sim_bench` outputs and those outputs vary between development machines and CI runners due to float differences, the training data has noise you can't control. For classical optimization (Phase 2) this is likely fine — variance washes out over 100 seeds. For RL with millions of runs (Phase 6), it could matter. The thermal system's milli-unit integer pattern is the right model, but migrating ~120 float fields is substantial. The honest question: is this a "do it all at once" migration or a "convert systems incrementally as you touch them" effort? Incremental conversion per-system (as each system gets major changes) is more realistic.

### Credit Assignment: The Phase 5→6 Gap

The gap between Phase 5 and Phase 6 is larger than it appears. Phase 5 ends with "a multi-station space economy with supply chains." Phase 6 starts with "LLM strategic advisor outputs autopilot config." The missing piece is evaluation: how does the LLM know its strategy was good? It needs to observe outcomes over thousands of ticks and correlate them with its decisions. That's a credit assignment problem that's genuinely hard.

The knowledge system helps (the playbook accumulates lessons), but automated feedback from "LLM made decision X → 5000 ticks later, outcome Y" needs explicit design. This is likely a Phase 5.5 deliverable: a structured outcome attribution system that traces strategic decisions to measurable results.

### The Feedback Loop Must Be Automated

Manual "run sim_bench, look at DuckDB, tweak config, repeat" doesn't scale. Phase 2's parameter sweep script is the first step toward automation. Each successive phase adds more automation — but the human should stay in the loop for validating that "higher score" actually means "more interesting gameplay," not just "found an exploit."

---

## Appendix: Linear Ticket Reference

### Code Quality Project (VIO-402 through VIO-419)

| Ticket | Priority | Title |
|--------|----------|-------|
| VIO-402 | High | Metrics pipeline: eliminate 12x-per-field duplication with derive macro |
| VIO-403 | High | Test fixture builders: reduce ~1,750 lines of copy-paste |
| VIO-404 | High | InventoryItem methods: collapse 136 match sites |
| VIO-405 | High | TradeItemSpec methods: collapse 7 functions × 3 match arms |
| VIO-406 | High | Constants override via serde: eliminate 200-line match |
| VIO-407 | Medium | ModuleBehaviorDef factory: eliminate triple-enum sync |
| VIO-408 | Medium | Generic module metrics: BTreeMap instead of per-type fields |
| VIO-409 | Medium | Decompose resolve_processor_run (~242 lines) |
| VIO-410 | Medium | Remove remaining clippy suppressions (7 production) |
| VIO-411 | Medium | Split types.rs (1,970 lines) into submodules |
| VIO-412 | Medium | Content-driven autopilot: replace 30+ hardcoded IDs |
| VIO-413 | Medium | Float determinism audit |
| VIO-414 | Medium | Add .clamp() guards to 7 unsafe casts |
| VIO-415 | Low | Extract 6 hardcoded constants to constants.json |
| VIO-416 | Low | Fix f64::EPSILON comparison in sim_events |
| VIO-417 | Low | Curate sim_core public exports |
| VIO-418 | Low | Naming improvements |
| VIO-419 | Low | Clean up sim_daemon test unwraps |

### Game Knowledge System Project (existing tickets)

| Ticket | Status | Title |
|--------|--------|-------|
| VIO-181 | Backlog | Design strategic advisor module architecture (3-layer system) |
| VIO-182 | Backlog | Supervised learning → RL pipeline (revised from MCTS) |
| VIO-179 | Backlog | Embed knowledge corpus with vector DB |
| VIO-180 | Backlog | RAG-enhanced balance recommendations |
| VIO-337–351 | Done | ML pipeline (Parquet, DuckDB, features, labels, cross-seed, stubs, smoke test) |
| VIO-173–178 | Done | Knowledge Phase 1 (schema, journals, playbook, MCP tools) |
