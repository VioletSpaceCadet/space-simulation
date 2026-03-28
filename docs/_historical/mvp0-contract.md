# MVP-0 Contract: "Scan → Data → Compute → Breakthrough"

This is the authoritative spec for MVP-0. Everything not listed here is out of scope.

**Out of scope for MVP-0:** HTTP daemon, SSE/WebSocket, React UI, travel mechanics, mining, hauling, refining, manufacturing, economy, multi-faction, LLM controller.

---

## Workspace Layout

```
Cargo.toml              (workspace root)
crates/
  sim_core/             (lib) — deterministic tick, no IO
  sim_control/          (lib) — command sources (autopilot, scenario)
  sim_cli/              (bin) — runs the simulation
content/
  techs.json
  solar_system.json
  asteroid_templates.json
  constants.json
saves/                  (gitignored, runtime output)
```

`sim_daemon` and `ui_web` are **not** created until MVP-1.

---

## Crate Dependency Graph

```
sim_cli → sim_core, sim_control
sim_control → sim_core
sim_core  (no internal deps)
```

---

## 1. sim_core Types

### Newtypes (ID wrappers)

All IDs are `struct Foo(pub String)` with `#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`.

- `ShipId`
- `AsteroidId`
- `StationId`
- `TechId`
- `NodeId`
- `SiteId`
- `CommandId`
- `EventId`
- `PrincipalId`

### Element and Composition

```rust
type ElementId = String;  // "Fe", "Si", "He"
type CompositionVec = HashMap<ElementId, f32>;  // fractions, should sum ~1.0
```

### Enums

```rust
enum AnomalyTag { IronRich }  // expand later

enum DataKind { ScanData }    // expand later

enum EventLevel { Normal, Debug }
```

### GameState

```rust
struct GameState {
    meta: MetaState,
    scan_sites: Vec<ScanSite>,                       // unscanned potential asteroid locations
    asteroids: HashMap<AsteroidId, AsteroidState>,   // populated on discovery
    ships: HashMap<ShipId, ShipState>,
    stations: HashMap<StationId, StationState>,
    research: ResearchState,
    counters: Counters,
}

struct MetaState {
    tick: u64,
    seed: u64,
    schema_version: u32,    // current: 1
    content_version: String,
}

struct ScanSite {
    id: SiteId,
    node: NodeId,
    template_id: String,    // which AsteroidTemplateDef to use on discovery
}

struct Counters {
    next_event_id: u64,
    next_command_id: u64,
}
```

The solar system graph is fully static and lives only in `GameContent`. No runtime world state is needed in MVP-0.

### AsteroidState

Asteroids only exist in state once discovered. There is no `discovered` flag.

```rust
struct AsteroidState {
    id: AsteroidId,
    location_node: NodeId,
    true_composition: CompositionVec,       // ground truth, never sent to UI
    anomaly_tags: Vec<AnomalyTag>,           // ground truth
    knowledge: AsteroidKnowledge,
}

struct AsteroidKnowledge {
    tag_beliefs: Vec<(AnomalyTag, f32)>,             // (tag, confidence 0..1)
    composition: Option<CompositionVec>,              // set on deep scan; exact, no uncertainty
}
```

### ShipState

```rust
struct ShipState {
    id: ShipId,
    location_node: NodeId,
    owner: PrincipalId,
    task: Option<TaskState>,
}
```

Fuel and cargo are **not** modelled in MVP-0.

### StationState

```rust
struct StationState {
    id: StationId,
    location_node: NodeId,
    power_available_per_tick: f32,
    facilities: FacilitiesState,
}

struct FacilitiesState {
    compute_units_total: u32,
    power_per_compute_unit_per_tick: f32,
    efficiency: f32,   // evidence per compute-unit per tick, baseline 1.0
}
```

Power buffer is not modelled in MVP-0; power is always available.

### ResearchState

Research distributes automatically to all eligible techs. There is no player allocation.

```rust
struct ResearchState {
    unlocked: HashSet<TechId>,
    data_pool: HashMap<DataKind, f32>,   // accumulates; not consumed in MVP-0
    evidence: HashMap<TechId, f32>,
}
```

### Tasks

```rust
struct TaskState {
    kind: TaskKind,
    started_tick: u64,
    eta_tick: u64,
}

enum TaskKind {
    Idle,
    Survey  { site: SiteId },
    DeepScan { asteroid: AsteroidId },
}
```

---

## 2. Command Types

```rust
struct CommandEnvelope {
    id: CommandId,
    issued_by: PrincipalId,
    issued_tick: u64,
    execute_at_tick: u64,
    command: Command,
}

enum Command {
    AssignShipTask {
        ship_id: ShipId,
        task_kind: TaskKind,
    },
}
```

`SetResearchAllocation` does not exist — research is fully automatic.

**Ownership rule:** a command affecting entity `E` is silently dropped if `E.owner != command.issued_by`.

---

## 3. Event Types

```rust
struct EventEnvelope {
    id: EventId,      // "evt_000001", monotonic per run
    tick: u64,
    event: Event,
}

enum Event {
    // Normal events
    TaskStarted   { ship_id: ShipId, task_kind: String, target: Option<String> },
    TaskCompleted { ship_id: ShipId, task_kind: String, target: Option<String> },
    AsteroidDiscovered { asteroid_id: AsteroidId },
    ScanResult    { asteroid_id: AsteroidId, tags: Vec<(AnomalyTag, f32)> },
    CompositionMapped { asteroid_id: AsteroidId, composition: CompositionVec },
    DataGenerated { kind: DataKind, amount: f32, quality: f32 },
    PowerConsumed { station_id: StationId, amount: f32 },
    TechUnlocked  { tech_id: TechId },

    // Debug only (emitted when EventLevel::Debug)
    ResearchRoll  { tech_id: TechId, evidence: f32, p: f32, rolled: f32 },
}
```

Event IDs are generated from `state.counters.next_event_id` (post-increment, zero-padded to 6 digits).

---

## 4. GameContent (static, loaded from JSON)

```rust
struct GameContent {
    content_version: String,
    techs: Vec<TechDef>,
    solar_system: SolarSystemDef,
    asteroid_templates: Vec<AsteroidTemplateDef>,
    constants: Constants,
}

struct TechDef {
    id: TechId,
    name: String,
    prereqs: Vec<TechId>,
    accepted_data: Vec<DataKind>,
    difficulty: f32,
    effects: Vec<TechEffect>,
}

enum TechEffect {
    EnableDeepScan,
    DeepScanCompositionNoise { sigma: f32 },   // noise applied when mapping composition
}

struct SolarSystemDef {
    nodes: Vec<NodeDef>,
    edges: Vec<(NodeId, NodeId)>,
}

struct NodeDef {
    id: NodeId,
    name: String,
}

struct AsteroidTemplateDef {
    id: String,
    anomaly_tags: Vec<AnomalyTag>,
    composition_ranges: HashMap<ElementId, (f32, f32)>,  // (min, max) per element
}

struct Constants {
    survey_scan_ticks: u64,
    deep_scan_ticks: u64,
    survey_scan_data_amount: f32,
    survey_scan_data_quality: f32,
    deep_scan_data_amount: f32,
    deep_scan_data_quality: f32,
    survey_tag_detection_probability: f32,  // per true tag, per survey
}
```

---

## 5. tick Function Signature and Contract

```rust
// in sim_core/src/lib.rs
pub fn tick(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    rng: &mut impl rand::Rng,
    event_level: EventLevel,
) -> Vec<EventEnvelope>
```

### Tick order (within one call)

1. **Apply commands** — filter to `execute_at_tick == state.meta.tick`; check ownership; mutate state.
2. **Advance ship tasks** — for each ship, if `task.eta_tick == state.meta.tick`, resolve it (see §6).
3. **Advance station research** — for each station, process all eligible techs (see §7).
4. **Increment tick** — `state.meta.tick += 1`.
5. **Return** accumulated `Vec<EventEnvelope>`.

### Determinism invariant

Given identical `(initial_state, commands, content, rng_seed_and_consumption_order)` → identical `(final_state, events)`.

---

## 6. Scan Task Mechanics

### Task assignment (step 1 — command applied)

When `AssignShipTask { ship_id, task_kind }` is applied:

```
ship.task = Some(TaskState {
    kind: task_kind,
    started_tick: state.meta.tick,
    eta_tick: state.meta.tick + duration,
})
```

Where `duration`:
- `TaskKind::Survey { .. }`   → `content.constants.survey_scan_ticks`
- `TaskKind::DeepScan { .. }` → `content.constants.deep_scan_ticks`

Emit `TaskStarted`.

For `DeepScan`: silently drop if `tech_deep_scan_v1` is not in `research.unlocked`.

### Survey scan completion (step 2)

When `eta_tick == current_tick` for a `Survey { site }` task:

1. Remove `site` from `state.scan_sites`.
2. Look up the site's `template_id`; find the matching `AsteroidTemplateDef` in content.
3. Roll a new `AsteroidState`:
   - Draw each element fraction from its `composition_range` using seeded RNG.
   - Copy `anomaly_tags` from the template.
   - Assign a new `AsteroidId` (e.g. `"asteroid_0001"` from a counter).
   - `knowledge = AsteroidKnowledge { tag_beliefs: [], composition: None }`.
4. Insert asteroid into `state.asteroids`.
5. Emit `AsteroidDiscovered`.
6. For each `anomaly_tag`: draw `rng.gen::<f32>() < survey_tag_detection_probability`; if detected, push `(tag, survey_tag_detection_probability)` onto `tag_beliefs`.
7. Emit `ScanResult` with detected tags.
8. `research.data_pool[ScanData] += survey_scan_data_amount * survey_scan_data_quality`
9. Emit `DataGenerated`.
10. Set `ship.task = Some(TaskState { kind: Idle, .. })`.
11. Emit `TaskCompleted`.

### Deep scan completion (step 2)

When `eta_tick == current_tick` for a `DeepScan { asteroid }` task:

1. Look up `TechEffect::DeepScanCompositionNoise { sigma }` from any unlocked tech's effects; default `sigma = 0.0`.
2. Set `asteroid.knowledge.composition`:
   - For each element: `mapped = true_value + rng.sample(Normal(0, sigma))`, clamped to `[0.0, 1.0]`.
   - Normalise the resulting map so fractions sum to 1.0.
3. Emit `CompositionMapped`.
4. `research.data_pool[ScanData] += deep_scan_data_amount * deep_scan_data_quality`
5. Emit `DataGenerated`.
6. Set `ship.task = Some(TaskState { kind: Idle, .. })`.
7. Emit `TaskCompleted`.

---

## 7. Research Mechanics (per tick, per station, automatic)

Research runs on all eligible techs simultaneously with no player allocation.

For each station:

1. Collect `eligible`: techs where prereqs are all in `research.unlocked` and tech is not yet unlocked.
2. If `eligible` is empty: nothing to do.
3. `per_tech_compute = station.facilities.compute_units_total as f32 / eligible.len() as f32`
4. `total_power = station.facilities.compute_units_total as f32 * station.facilities.power_per_compute_unit_per_tick`
5. Emit `PowerConsumed { station_id, amount: total_power }`.
6. For each eligible tech:
   - `research.evidence[tech] += per_tech_compute * station.facilities.efficiency`
   - `p = 1.0 - (-research.evidence[tech] / tech.difficulty).exp()`
   - `rolled = rng.gen::<f32>()`
   - If `EventLevel::Debug`: emit `ResearchRoll`.
   - If `rolled < p`: emit `TechUnlocked`, insert into `research.unlocked`.

---

## 8. sim_control: CommandSource Trait

```rust
// in sim_control/src/lib.rs
pub trait CommandSource {
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}
```

### AutopilotController

Per tick, for each ship owned by `PrincipalId("principal_autopilot")` with task `Idle`:

1. If any `ScanSite` exists in `state.scan_sites` → issue `AssignShipTask(Survey { site })` for the first one.
2. Else if `tech_deep_scan_v1` is unlocked and any asteroid has `knowledge.composition == None` and has `IronRich` confidence > 0.7 → issue `AssignShipTask(DeepScan { asteroid })`.
3. Else → do nothing.

Research needs no autopilot action — it is fully automatic.

### ScenarioSource

Reads a JSON file mapping `tick → Vec<Command>` and emits commands at the right tick. Used for deterministic scenario tests.

---

## 9. World Generation (CLI, not sim_core)

The initial `GameState` is constructed by `sim_cli` using content + seed:

1. Instantiate `station_earth_orbit` at `"node_earth_orbit"`.
2. Instantiate `ship_0001` at `"node_earth_orbit"`, owned by `"principal_autopilot"`, task = Idle.
3. For each template × `count_per_template`, generate a `ScanSite` with a random node from the solar system graph (using seeded RNG). Assign IDs `site_0001`, `site_0002`, ...
4. `state.asteroids` starts empty.
5. `research = ResearchState { unlocked: {}, data_pool: {}, evidence: {} }`.
6. `meta.tick = 0`, `meta.seed = seed`.

---

## 10. Persistence Format

```
saves/<run_id>/
  meta.json       — { seed, content_version, schema_version, run_id }
  state.json      — full GameState snapshot (overwritten every N ticks)
  events.jsonl    — one EventEnvelope per line, append-only
  commands.jsonl  — one CommandEnvelope per line, append-only
```

`run_id` format: `run_YYYYMMDD_HHMMSS`.

State is flushed every `save_interval_ticks` (CLI arg, default 1000). Always flushed on normal exit.

---

## 11. sim_cli Interface

```
sim_cli run \
  --ticks <u64>                   # required
  --seed <u64>                    # required
  --save-dir <path>               # default: ./saves
  --print-every <u64>             # default: 100
  --event-level <normal|debug>    # default: normal

sim_cli inspect --save <path> --tick <u64>
```

### `run` stdout (every `print_every` ticks)

```
[tick=0100  day=0  hour=1]  sites_remaining=17  asteroids=3  unlocked=[]  data={ScanData: 42.0}
```

On tech unlock:
```
*** TECH UNLOCKED: tech_deep_scan_v1 at tick=0843 ***
```

### `inspect`

Print full state at the given tick. If no snapshot for that exact tick, use the nearest earlier one and note the gap.

---

## 12. Content JSON (MVP-0 files)

### `content/constants.json`

```json
{
  "survey_scan_ticks": 10,
  "deep_scan_ticks": 30,
  "survey_scan_data_amount": 5.0,
  "survey_scan_data_quality": 1.0,
  "deep_scan_data_amount": 15.0,
  "deep_scan_data_quality": 1.2,
  "survey_tag_detection_probability": 0.85
}
```

### `content/techs.json`

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
        { "type": "EnableDeepScan" },
        { "type": "DeepScanCompositionNoise", "sigma": 0.02 }
      ]
    }
  ]
}
```

### `content/solar_system.json`

```json
{
  "nodes": [
    { "id": "node_earth_orbit", "name": "Earth Orbit" },
    { "id": "node_belt_inner",  "name": "Inner Belt" },
    { "id": "node_belt_mid",    "name": "Mid Belt" },
    { "id": "node_belt_outer",  "name": "Outer Belt" }
  ],
  "edges": [
    ["node_earth_orbit", "node_belt_inner"],
    ["node_belt_inner",  "node_belt_mid"],
    ["node_belt_mid",    "node_belt_outer"]
  ]
}
```

### `content/asteroid_templates.json`

```json
{
  "count_per_template": 10,
  "templates": [
    {
      "id": "tmpl_iron_rich",
      "anomaly_tags": ["IronRich"],
      "composition_ranges": {
        "Fe": [0.55, 0.80],
        "Si": [0.10, 0.30],
        "He": [0.00, 0.15]
      }
    },
    {
      "id": "tmpl_silicate",
      "anomaly_tags": [],
      "composition_ranges": {
        "Fe": [0.10, 0.25],
        "Si": [0.55, 0.80],
        "He": [0.00, 0.10]
      }
    }
  ]
}
```

Two templates × 10 = 20 scan sites. Fine for MVP-0.

---

## 13. MVP-0 Success Criteria

After `sim_cli run --ticks 10000 --seed 42`:

- All 20 scan sites are surveyed (survey scans ran to completion).
- `ScanData` accumulated in data pool.
- `tech_deep_scan_v1` unlocked at some tick (with seed 42, the unlock tick becomes a regression anchor).
- After unlock, deep scans run on IronRich asteroids and `composition` is set.
- `state.json`, `events.jsonl`, `commands.jsonl` are written.
- Re-running with `--seed 42` produces byte-identical `events.jsonl`.

---

## 14. External Crates

| Crate | Used in | Purpose |
|---|---|---|
| `serde` + `serde_json` | all | serialization |
| `rand` | sim_core, sim_cli | RNG (trait in sim_core, impl in sim_cli) |
| `rand_chacha` | sim_cli | seeded, portable RNG |
| `anyhow` | sim_cli | error handling |
| `clap` | sim_cli | CLI args |

`sim_core` depends only on `serde` and `rand` (trait only, no concrete RNG impl).
