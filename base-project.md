# Space Industry Sim v0 Design Doc

*A deterministic, simulation-first space mining/refining/manufacturing prototype with CLI + React “mission control” observer UI.*

---

## 0) Goals and Non-Goals

### Goals (v0–v1)

* Deterministic simulation core in Rust (`sim_core`) with a fixed **1-minute tick**.
* Simulation runs headless via CLI and/or daemon; produces **events** and **state snapshots**.
* Early gameplay is **automation + scenarios**, not player-driven UI.
* Research is a **throughput system** (data + compute + energy), with **probabilistic breakthroughs**.
* Exploration/scanning with **anomalies** and **progressive composition discovery**.
* Extensible architecture: add mining, transport, refining, manufacturing, economy over time.

### Non-Goals (initial)

* Real orbital mechanics or high-fidelity physics.
* Complex graphics or engine integration.
* Multiplayer.
* Full production economy simulation (markets can start fixed).
* Full “quality matrices” for manufactured goods (optional later).

---

## 1) System Overview (Layers)

### Layer 1: React UI (Observer)

* “Mission control” dashboard: read-only initially.
* Consumes snapshots + streamed events.
* Later can emit player commands (POST).

### Layer 2: Rust Daemon (`sim_daemon`)

* Orchestrator: loads config/content, loads/creates save, runs tick loop.
* Calls `sim_core::tick(...)`.
* Persists `state.json`, appends `events.jsonl` and `commands.jsonl`.
* Serves HTTP endpoints and event stream (SSE or WebSocket).

### Layer 3: Rust Simulation Core (`sim_core`)

* Deterministic tick function: mutates state and returns events.
* All gameplay rules live here (scan, research, mining, refining, etc).
* No IO, no network, no DB.

### Layer 4: Runtime Persistence (“State DB”)

* `state.json` (latest snapshot)
* `events.jsonl` (append-only log of events)
* `commands.jsonl` (append-only log of commands that were applied)

### Layer 5: Static Content (“Config DB”)

* JSON files in `content/`:

  * tech tree, resources, processes, templates, solar system graph, ship archetypes, constants

---

## 2) Time Model

* Tick duration: **1 minute**.
* Canonical sim time: `tick_index: u64`.
* Derived display:

  * `minutes = tick_index`
  * `hours = tick_index / 60`
  * `days = tick_index / 1440`
* Optional later: map tick to calendar date via base epoch.

---

## 3) ID Strategy

Use human-readable string IDs (no UUIDs initially):

* `ship_0001`, `asteroid_0042`, `station_earth_orbit`, `tech_deep_scan_v1`
* Tech IDs and recipe IDs are stable names.

### Rust typed wrappers (recommended)

* `ShipId(String)`, `AsteroidId(String)`, `StationId(String)`, `TechId(String)`, etc.
* Avoid mixing IDs accidentally.

---

## 4) Core Simulation Contract

### Inputs to `tick`

* `&mut GameState`
* `&[CommandEnvelope]` (commands that execute now)
* `&GameContent` (static JSON content)
* `&mut Rng` (seeded RNG owned by daemon)

### Outputs from `tick`

* `Vec<EventEnvelope>` describing what happened (facts).
* **Events are not instructions**. The daemon does not “apply” them to state.

### Determinism Requirement

Given:

* identical initial state
* identical command stream (same commands at same ticks)
* identical content version
* identical RNG seed and consumption order
  Then:
* identical resulting states and event stream.

---

## 5) Entity Model (GameState)

Store entities in flat maps keyed by ID.

### `GameState` (minimum viable)

* `meta`

  * `tick: u64`
  * `seed: u64`
  * `schema_version: u32`
  * `content_version: String`
* `world`

  * `solar_system`: node/edge travel graph state (static edges from content; runtime can track discovery)
* `asteroids: HashMap<AsteroidId, AsteroidState>`
* `ships: HashMap<ShipId, ShipState>`
* `stations: HashMap<StationId, StationState>`
* `research: ResearchState`
* `economy: EconomyState` (v0 minimal / stub ok)
* `counters` (optional): ID generators, global stats

### `AsteroidState` (v0)

* `id`
* `location_node: NodeId` (graph node)
* `true_composition: CompositionVec` (ground truth)
* `anomaly_tags: Vec<AnomalyTag>` (ground truth)
* `discovered: bool`
* `knowledge: AsteroidKnowledge` (player/faction estimate)

  * `tag_beliefs: Vec<(AnomalyTag, confidence: f32)>`
  * `composition_estimate: Option<CompositionEstimate>`

    * `mean: CompositionVec`
    * `uncertainty: CompositionUncertainty` (per component or scalar)

### `ShipState` (v0)

* `id`
* `location_node: NodeId`
* `fuel: f32`
* `cargo: Inventory` (can be empty v0)
* `owner: PrincipalId` (who can command it)
* `task: Option<TaskState>`

### `StationState` (v0)

* `id`
* `location_node: NodeId`
* `power_available_per_tick: f32`
* `power_buffer: f32` (optional later)
* `facilities: FacilitiesState`

  * compute labs count, refinery modules later
* `inventory: Inventory` (v0 optional)

### `ResearchState` (v0)

* `unlocked: HashSet<TechId>`
* `data_pool: HashMap<DataKind, f32>` (accumulated “data points”)
* `active_allocations: HashMap<TechId, ComputeAllocation>` (compute units assigned)
* `evidence: HashMap<TechId, f32>` (accumulated evidence)

---

## 6) Composition & Materials Model

### Composition vector (mixtures)

* Represent as map `ElementId -> fraction` or dense vec with fixed element ordering from content.
* v0 elements: `Fe`, `Si`, `He` (expand later).

### Two material categories

1. **Mixtures**: composition vector tracked precisely (ore, slag, concentrate).
2. **Products**: discrete commodities with optional simple quality (later).

   * v0 can ignore product quality.

This prevents composition math from infecting downstream manufacturing.

---

## 7) Tasks (Ongoing Processes)

Tasks are first-class and stored on ships/facilities.

### Task design principles

* Commands set or modify tasks.
* Tick advances tasks deterministically.
* Task progress is explicit.
* Task completion emits events.

### `TaskState` (v0)

* `kind: TaskKind`
* `target: Option<EntityId>`
* `started_tick: u64`
* `eta_tick: Option<u64>` or `progress: f32`
* `params: TaskParams` (mode, intensity, etc.)

### v0 TaskKinds

* `Idle`
* `Travel { to: NodeId }` (optional v0)
* `Scan { asteroid: AsteroidId, mode: ScanMode }`

Later:

* `Mine`, `Haul`, `RefineBatch`, `ManufactureBatch`, `ComputeResearch` (station-level)

---

## 8) Command Model

Commands are *requests* issued by controllers (scenario, automation, future player).

### Command envelope

* `id: CommandId` (string like `cmd_000001`)
* `issued_by: PrincipalId` (e.g. `principal_autopilot`, `principal_scenario`)
* `issued_tick: u64`
* `execute_at_tick: u64` (usually `now`)
* `command: Command`

### v0 Commands (minimum)

* `AssignShipTask { ship_id, task }`
* `SetResearchAllocation { station_id, allocations: Vec<(TechId, compute_units)> }`
* `SetEventVerbosity { level }` (daemon-only, not sim command; optional)

### Ownership rule

* Commands affecting an entity are applied only if `entity.owner == command.issued_by` (or by governance policy).

---

## 9) Event Model

Events are facts produced by the sim. They drive:

* UI updates
* debugging
* replay introspection

### Event envelope

* `id: EventId` (monotonic `evt_000001` per run)
* `tick: u64`
* `event: Event`

### v0 Events (minimum set)

* `TaskStarted { ship_id, task_kind, target }`
* `TaskCompleted { ship_id, task_kind, target }`
* `AsteroidDiscovered { asteroid_id }`
* `ScanResult { asteroid_id, tags: Vec<(AnomalyTag, confidence)> }`
* `CompositionEstimateUpdated { asteroid_id, estimate: CompositionEstimate }`
* `DataGenerated { kind: DataKind, amount: f32, quality: f32, tags: Vec<String> }`
* `PowerConsumed { station_id, amount: f32 }`
* `TechUnlocked { tech_id }`
* Optional debug:

  * `ResearchRoll { tech_id, evidence, p, rolled }`

### Event verbosity

* The sim can accept a runtime flag (from daemon) like `EventLevel`:

  * `Normal`: only major events
  * `Debug`: add instrumentation events

---

## 10) Research System (Throughput + Probabilistic Breakthrough)

### Concept

* Gameplay actions generate **data**.
* Compute labs consume energy to convert data into **evidence** on tech projects.
* Tech unlocks happen probabilistically based on evidence and difficulty.
* Multiple techs can be researched simultaneously (compute allocation).

### Data generation (v0)

* Scanning generates `ScanData`.
* Travel later generates `TelemetryData`.
* Mining later generates `MiningData`.

`ResearchState.data_pool[ScanData] += amount * quality`

### Compute labs (v0)

Each station has:

* `compute_units_total: u32`
* `power_per_compute_unit_per_tick: f32`
* `efficiency: f32` (evidence per compute per tick, baseline 1.0)

Allocation:

* `alloc[tech_id] = compute_units`

Per tick, for each allocated tech:

* Consume power: `compute_units * power_per_unit`
* Consume data: can be:

  * simple: require minimum data exists; consume `data_cost_per_evidence`
  * v0 KISS: do not consume data, just require it exists above threshold; or consume a small amount

Evidence accumulation:

* `evidence[tech] += compute_units * efficiency * data_quality_factor`

`data_quality_factor` v0:

* compute from data pool quality; simplest is 1.0, improve later.

### Breakthrough chance (v0-friendly)

Use an increasing probability curve with diminishing returns.

Option A (simple and good):

* `p = 1 - exp(-evidence / difficulty)`
* Each tick roll once per tech: unlock if `rng < p`

Option B (even simpler):

* If `evidence > difficulty`, then each tick roll with `p = base + k*(evidence - difficulty)` capped.
* This creates a “soft threshold.”

Choose A for smoother behavior and good feel.

### Tech prerequisites

Tech unlock only if all prereqs unlocked.

If prereqs unmet:

* evidence can either:

  * not accumulate (simplest), or
  * accumulate in “buffer” but no unlock until prereqs met (more flexible)
    v0 recommendation: **no accumulation until prereqs met**.

---

## 11) Exploration, Anomalies, Knowledge

### Asteroid generation

* Asteroids have ground truth:

  * anomaly tags
  * composition vector
* Knowledge starts unknown.
* Scans reveal:

  * tags with confidence
  * composition estimate with uncertainty (deep scans reduce uncertainty)

### Scan modes

* `ScanMode::Survey` (fast, tag discovery)
* `ScanMode::Deep` (slower, composition estimate improvement; may require tech)

Mechanics (v0 KISS):

* Survey scan:

  * sets `discovered=true`
  * emits `AsteroidDiscovered`
  * emits `ScanResult` with tag confidence
  * generates `ScanData`
* Deep scan:

  * updates `composition_estimate` mean closer to truth and reduces uncertainty
  * generates more `ScanData`

Uncertainty reduction can be a simple scalar:

* `uncertainty = max(uncertainty * (1 - k), min_uncertainty)`
* where `k` depends on sensor strength + tech.

---

## 12) Control Layer (Command Sources)

Control is isolated from daemon and sim core logic.

### Command sources (pluggable)

* `ScenarioSource` (reads scripted commands)
* `AutopilotController` (simple heuristics)
* `PlayerSource` (later: HTTP POST)
* `FactionAI` (later)
* `LLMStrategist` (later; non-deterministic allowed if commands are logged)

### Governance policy (v0)

* Single owner per controllable entity.
* Only owner’s commands apply.
* Controllers can run in priority order; conflicts are dropped with a log message.

### v0 Autopilot behavior (minimal)

* If a ship is idle:

  * pick an undiscovered asteroid and survey scan it
  * if survey says IronRich confidence > threshold:

    * if DeepScan tech unlocked: assign deep scan
    * else allocate research to DeepScan tech

Keep it dumb and predictable.

---

## 13) Persistence & Save Format

### Folder structure

`saves/<run_id>/`

* `meta.json` (seed, content version, schema version)
* `state.json` (full snapshot)
* `events.jsonl` (append-only)
* `commands.jsonl` (append-only)

### Save semantics

* Daemon persists `state.json` every N ticks (configurable).
* Always append events and commands as they occur.
* On startup:

  * load latest `state.json`
  * optionally replay commands/events since last snapshot (future optimization)

### Versioning

Include:

* `schema_version` in state and meta
* `content_version` (hash or semver) of content pack used
* A load strategy for mismatch:

  * v0: refuse to load mismatched content unless `--force`

---

## 14) Daemon API for React UI (Read-Only v0)

### Endpoints (minimum)

* `GET /api/v1/snapshot`

  * returns current `GameState` snapshot (or a trimmed `SnapshotView`)
* `GET /api/v1/events?after=<event_id>`

  * returns events after ID
* `GET /api/v1/meta`

  * returns tick, run id, schema/content versions

### Streaming (recommended)

* SSE: `GET /api/v1/stream`

  * sends event envelopes as they occur
  * optional periodic heartbeat with tick/time

WebSocket is also fine, SSE is simpler.

### Future (write APIs)

* `POST /api/v1/commands`

  * enqueue player commands
  * still logged to `commands.jsonl`

---

## 15) CLI Tools

### `sim_cli` commands

* `run --scenario scenario.json --ticks 10000 --seed 42`
* `inspect --save saves/run_001 --tick 1440`
* `replay --save saves/run_001` (replay command log)
* `summarize --save ...` (topline metrics)

CLI output should be structured and not endless:

* print every X ticks:

  * tick/day/hour
  * discoveries count
  * latest tech unlocked
  * research evidence + allocations
  * power usage

---

## 16) Testing Strategy

### Unit tests (sim_core heavy)

* Scanning updates discovery + knowledge correctly.
* Uncertainty reduction behaves.
* Research evidence accumulation and power consumption.
* Probabilistic unlock sanity checks:

  * with fixed seed, unlock at expected tick
  * distribution tests with multiple seeds (property tests)

### Scenario tests (high value)

Scripted scenario that asserts:

* after survey scans, IronRich tag discovered
* after compute investment, DeepScan unlocks eventually
* deep scan reduces uncertainty below threshold

### Integration tests (daemon)

* Save/load roundtrip.
* API endpoints return consistent snapshot.
* Event stream ordering and resume via `after_id`.

### Determinism tests

* Run same seed + same commands twice, compare:

  * final state hash
  * event log hash

---

## 17) Repo Layout (recommended)

```
/crates
  /sim_core
  /sim_control
  /sim_daemon
  /sim_cli
/content
  techs.json
  resources.json
  solar_system.json
  asteroid_templates.json
  ships.json
  constants.json
/saves
  /run_0001
/ui_web
  (React app)
```

---

## 18) MVP Build Plan (Freeze This)

### MVP-0: “Scan → Data → Compute → Breakthrough”

* 1 station: `station_earth_orbit`

  * power budget
  * compute lab
* 1 ship: `ship_0001` (scanner)
* 10–50 asteroids with ground-truth composition + anomaly tags
* Tech: `tech_deep_scan_v1`

  * prereqs: none
  * accepts: `ScanData`
  * unlock effect: enables DeepScan mode or improves deep scan effectiveness
* Autopilot:

  * survey scan unknown asteroids
  * invest compute into DeepScan tech
  * deep scan iron-rich once unlocked

### Success criteria

* After running 1,000–10,000 ticks:

  * some asteroids discovered
  * scan data accumulated
  * compute consumes power
  * `tech_deep_scan_v1` unlocks probabilistically
  * composition uncertainty decreases after deep scans
* React UI can show:

  * timeline of events
  * discovered asteroids table with tag confidence and composition estimate
  * research panel with allocations and unlock status

---

## 19) Future Extensions (Post-MVP Roadmap Hooks)

### Mining/Transport

* Add tasks: `Mine`, `Haul`
* Add materials as mixtures in inventory lots
* Generate `MiningData`, `TelemetryData`

### Refining Separation

* Add refinery facility and `RefineBatch` task
* Output streams: concentrate + slag + volatiles capture
* Keep slag composition and allow future reprocessing

### Manufacturing

* Convert mixture outputs to product commodities
* Optional product quality tiers (poor/good/excellent) later

### Economy

* Fixed Earth market buys/sells with “launch cost”
* Later: multiple markets and contracts

### Energy/Fuel

* Keep early KISS, then add generators/storage/constraints

### AI/Agentic Controllers

* Add Intent layer: `Intent -> Commands` compiler
* Optional LLM strategist that proposes intents; commands logged for determinism

---

## 20) Implementation Notes / Rules of Thumb

* **Daemon must stay boring.** If code feels like gameplay, move it to `sim_core`.
* Prefer **flat entity maps** to avoid borrow-checker headaches.
* Avoid event spam: log only meaningful deltas; keep micro-state in snapshot.
* Always log commands. Even automation commands. This makes replay and debugging easy.
* Keep content JSON stable; evolve with versioning.

---

## Appendix A: Minimal Content JSON Sketches

### `techs.json` (example)

```json
{
  "content_version": "0.0.1",
  "techs": [
    {
      "id": "tech_deep_scan_v1",
      "name": "Deep Scan v1",
      "prereqs": [],
      "accepted_data": ["ScanData"],
      "difficulty": 200.0,
      "effects": [
        { "type": "EnableScanMode", "mode": "Deep" },
        { "type": "DeepScanUncertaintyMultiplier", "multiplier": 0.85 }
      ]
    }
  ]
}
```

### `asteroid_templates.json` (example)

```json
{
  "templates": [
    {
      "id": "tmpl_iron_rich",
      "anomaly_tags": ["IronRich"],
      "composition_ranges": {
        "Fe": [0.55, 0.80],
        "Si": [0.10, 0.30],
        "He": [0.00, 0.20]
      }
    }
  ]
}
```

---

## Appendix B: Minimal API Payloads (example)

### `GET /snapshot`

```json
{
  "schema_version": 1,
  "content_version": "0.0.1",
  "tick": 4320,
  "stations": { "...": "..." },
  "ships": { "...": "..." },
  "asteroids": { "...": "..." },
  "research": { "...": "..." }
}
```

### `events.jsonl` line example

```json
{"id":"evt_000123","tick":4320,"event":{"type":"TechUnlocked","tech_id":"tech_deep_scan_v1"}}
```

---

If you want, I can also produce a **“MVP Contract”** as a smaller, stricter subset (exact structs/enums and file schemas) that you can hand to Claude/Codex as the authoritative source while they generate the repo.
