# Sim Events System Design

**Goal:** Content-driven event engine with composable effects, deterministic evaluation, and extensible primitives for future choices, chains, and scripted events.
**Status:** Planned
**Linear Project:** [Sim Events System](https://linear.app/violetspacecadet/project/sim-events-system-9f033907b744)

## Overview

The simulation is fully deterministic — same seed produces identical outcomes. While determinism is non-negotiable, the game lacks unpredictability that forces adaptation. Sim events (natural hazards, discoveries, equipment failures, cosmic phenomena) create pressure, narrative, and replayability.

This design builds the **event engine primitives** — the generic system for defining, selecting, targeting, and applying events entirely from content JSON. The engine is content-agnostic: all behavior comes from JSON definitions, not hardcoded logic. No special-casing for specific event IDs in the evaluation engine. If event-specific behavior is needed, the primitives must be extended rather than adding exceptions.

This property is deliberate. The event system is the first subsystem in the sim that is entirely content-defined. The condition/effect/targeting pattern is general enough to express behaviors beyond random events and serves as the prototype for what a fully data-driven sim layer looks like.

## Design

### Data Model

#### New module: `sim_core/src/sim_events.rs`

**Newtype ID:**
```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventDefId(pub String);
```

**Event definition (engine-relevant fields only — no description_template):**
```rust
pub struct SimEventDef {
    pub id: EventDefId,
    pub name: String,                          // Display label (like ModuleDef.name)
    pub category: String,                      // "natural", "operational", "discovery"
    pub tags: Vec<String>,                     // ["hazard", "damage"]
    pub rarity: Rarity,                        // Common/Uncommon/Rare/Legendary
    pub weight: u32,                           // Resolved from rarity or weight_override
    pub cooldown_ticks: u64,                   // Per-event cooldown
    pub conditions: Vec<Condition>,            // All must be true (implicit AND)
    pub weight_modifiers: Vec<WeightModifier>, // Dynamic weight adjustments
    pub targeting: TargetingRule,              // Who gets hit
    pub effects: Vec<EffectDef>,              // What happens (applied sequentially, order matters)
}
```

**Rarity → weight mapping (resolved at load time, not stored twice):**
- Common = 100, Uncommon = 25, Rare = 5, Legendary = 1
- Content JSON: `{ "rarity": "rare" }` → weight=5
- Content JSON: `{ "rarity": "rare", "weight_override": 8 }` → weight=8
- Only `weight` stored on the struct.

**Condition (simple field comparisons, extensible to boolean trees later):**
```rust
pub struct Condition {
    pub field: ConditionField,  // Tick, StationCount, ShipCount, AvgModuleWear, Balance, TechsUnlockedCount, ...
    pub op: CompareOp,          // Gt, Gte, Lt, Lte, Eq
    pub value: f64,
}
```

The flat list of conditions is an implicit AND (all must be true). Upgrading to explicit AND/OR trees later is a backward-compatible schema extension via `#[serde(untagged)]` — the inner `{ field, op, value }` struct stays the same.

**WeightModifier (dynamic weight based on game state, integer arithmetic):**
```rust
pub struct WeightModifier {
    pub condition: Condition,
    pub weight_multiplier_pct: u32,  // 200 = 2x, 50 = 0.5x, 0 = remove from pool
}
```

**TargetingRule:**
```rust
pub enum TargetingRule {
    Global,
    RandomStation,
    RandomShip,
    RandomModule { station: TargetStation },
    Zone { zone_id: Option<String> },
}

pub enum TargetStation {
    Random,
    MostModules,
    HighestWear,
}
```

**ResolvedTarget:**
```rust
pub enum ResolvedTarget {
    Global,
    Station(StationId),
    Ship(ShipId),
    Module { station_id: StationId, module_id: ModuleId },
    Zone(String),
}
```

**EffectDef (generic, composable):**
```rust
pub enum EffectDef {
    DamageModule { wear_amount: f32 },
    AddInventory { item: TradeItemSpec },
    AddResearchData { domain: String, amount: f64 },
    SpawnScanSite { zone_override: Option<String>, template_override: Option<String> },
    ApplyModifier { stat: StatId, op: ModifierOp, value: f64, duration_ticks: u64 },
    TriggerAlert { severity: String, message: String },
}
```

Effects apply sequentially in definition order — ordering matters. A comment in the code documents this contract.

**GameState addition:**
```rust
// On GameState (with #[serde(default)] for backward compatibility):
pub events: SimEventState,
```

```rust
pub struct SimEventState {
    pub history: VecDeque<FiredEvent>,              // Ring buffer, capped
    pub cooldowns: BTreeMap<EventDefId, u64>,       // BTreeMap for deterministic iteration
    pub global_cooldown_until: u64,                 // Next tick any event can fire
    pub active_effects: Vec<ActiveEffect>,          // Temporary modifiers awaiting expiry
}

pub struct FiredEvent {
    pub event_def_id: EventDefId,
    pub tick: u64,
    pub target: ResolvedTarget,
    pub effects_applied: Vec<EffectDef>,
}

pub struct ActiveEffect {
    pub source_event_id: EventDefId,
    pub target_entity: EffectTarget,    // Which entity's ModifierSet
    pub expires_at_tick: u64,
}
```

**ModifierSource extension:**
```rust
pub enum ModifierSource {
    Environment,
    Equipment(String),
    Tech(String),
    Thermal,
    Wear,
    Event(EventDefId),   // NEW
}
```

**Constants additions (`constants.json`):**
```json
{
  "events_enabled": true,
  "event_global_cooldown_ticks": 200,
  "event_history_capacity": 100
}
```

All with `#[serde(default)]` for backward compatibility.

### Tick Ordering

New tick phase 4.5 — after `advance_research()` (4), before `replenish_scan_sites()` (5):

```
1. apply_commands()
2. resolve_ship_tasks()
3. tick_stations() (3.1-3.8)
4. advance_research()
4.5 evaluate_events()     ← NEW
5. replenish_scan_sites()
6. state.meta.tick += 1
```

**`evaluate_events()` flow:**

1. **Guard**: if `!constants.events_enabled`, return immediately.
2. **Sweep expired effects**: if `!state.events.active_effects.is_empty()`, remove expired modifiers from target entities' ModifierSets, emit `SimEventExpired` events.
3. **Check global cooldown**: if `tick < global_cooldown_until`, return.
4. **Build candidate pool**: iterate event defs sorted by ID (determinism). Filter: all conditions pass, per-event cooldown not active. Compute effective weight: `base_weight * product(applicable_multiplier_pct) / 100^n` (integer arithmetic). Skip if effective weight == 0.
5. If pool empty, return.
6. **Weighted random selection**: cumulative integer weights, `rng.gen_range(0..total_weight)`.
7. **Resolve target**: sort entities by ID, pick via RNG.
8. **Apply effects** sequentially in definition order:
   - `DamageModule` → add wear to target module's WearState
   - `AddInventory` → add items to target station inventory
   - `AddResearchData` → add to ResearchState.data_pool
   - `SpawnScanSite` → push new ScanSite (counts toward replenish cap)
   - `ApplyModifier` → add Modifier (with `ModifierSource::Event`) to target entity's ModifierSet, push ActiveEffect
   - `TriggerAlert` → emit AlertRaised event
9. **Record** FiredEvent in history ring buffer. Update per-event cooldown. Set global cooldown.
10. **Emit** `SimEventFired` SSE event.

**Determinism guarantees:**
- Event defs sorted by ID before pool evaluation
- Entities sorted by ID before target selection
- All RNG calls through passed-in `&mut impl Rng`
- Integer weight arithmetic (no float rounding variance)
- BTreeMap cooldowns iterated in sorted order
- Effects applied in definition vec order (deterministic, documented)

### SSE / API / Frontend Integration

**New Event variants:**
```rust
Event::SimEventFired {
    event_def_id: EventDefId,
    target: ResolvedTarget,
    effects_applied: Vec<AppliedEffect>,
}

Event::SimEventExpired {
    event_def_id: EventDefId,
    effects_removed: Vec<AppliedEffect>,
}
```

**Dual emission contract (documented in code):**
- `SimEventFired` = narrative event for the events feed (what happened and to whom)
- Individual effect events (WearAccumulated, AlertRaised, ScanSiteSpawned) = mechanical updates for existing UI handlers
- Both emitted, neither redundant. If a future developer deduplicates, things break.

**API changes:**
- `GET /api/v1/content` — add event defs with presentation fields (name, description_template, category, tags) for frontend template interpolation.
- `GET /api/v1/state` — `SimEventState` serialized as part of GameState.
- No new endpoints.

**Frontend:**
- `applyEvents.ts` — handlers for `SimEventFired` and `SimEventExpired`. Updates local event history state.
- `eventSchemas.ts` — Zod schemas for new event variants.
- `EventsFeed.tsx` — display sim events with category-based styling. Template interpolation: look up `description_template` from content, substitute `{target}` with resolved target name. Fallback: display `event_def_id` if template not found.
- `ci_event_sync.sh` — add new Event variants.
- No new panel in Phase 1.

### Content Files

**New file: `content/events.json`**

6 initial events covering all effect types:

| Event | Rarity | Effects | Category |
|---|---|---|---|
| Critical Equipment Failure | Uncommon | DamageModule, TriggerAlert | operational |
| Comet Flyby | Rare | SpawnScanSite (×2), TriggerAlert | discovery |
| Supernova Observation | Rare | AddResearchData, TriggerAlert | discovery |
| Solar Flare | Uncommon | ApplyModifier (temp +wear_rate), TriggerAlert | natural |
| Micrometeorite Shower | Common | DamageModule (small), ApplyModifier (temp) | natural |
| Supply Cache Discovery | Rare | AddInventory (repair_kits), TriggerAlert | discovery |

**Condition examples:**
- Equipment Failure: `station_count >= 1`, weight increases 3x when `avg_module_wear > 0.6`
- Comet Flyby: `tick >= 1000`
- Solar Flare: `station_count >= 1`

**Content validation at load time:**
- Event def IDs unique
- Condition fields reference valid ConditionField variants
- Effect-targeting coherence: DamageModule requires Station/Module target, AddInventory requires Station target, SpawnScanSite requires Zone/Global target, AddResearchData is Global-only
- weight_multiplier_pct must be non-negative
- Cooldown ticks > 0
- Panic on authoring errors (consistent with existing validation pattern)

### Migration / Backwards Compatibility

- `GameState.events: SimEventState` — `#[serde(default)]` → old saves load with empty history, no cooldowns, no active effects.
- `ModifierSource::Event(EventDefId)` — existing serialized ModifierSets with no Event modifiers unaffected.
- `Constants` — new fields with `#[serde(default)]`: `events_enabled=true`, `event_global_cooldown_ticks=200`, `event_history_capacity=100`.
- `content/events.json` — missing file = empty event pool (no events, no panic). Graceful degradation.
- sim_bench — scenarios can disable events: `"overrides": { "events_enabled": false }`.

**No breaking changes.** Old saves load. Old scenarios run. Events are purely additive.

## Testing Plan

- **Unit tests** (sim_core): condition evaluation, weight computation (integer math), effect application for each EffectDef variant, cooldown logic, expiry sweep, history ring buffer capacity
- **Determinism regression test**: `events_test.json` sim_bench scenario with low cooldowns. Run same seed twice, assert identical event sequences and final state. This is the primary determinism safeguard for the event pipeline.
- **Integration test**: full pipeline with real content — fire event → apply effects → emit SSE events → verify state changes
- **Effect-targeting coherence**: validate at load time that effects are compatible with targeting rules
- **sim_bench scenario**: `scenarios/events_test.json` with `event_global_cooldown_ticks: 10` for high-frequency event observation
- **Mutation testing**: `cargo-mutants` for condition evaluation, weight computation, effect application paths

## Ticket Breakdown

### Phase 1: Sim Events Engine

1. **SE-01: Data model + content loading** — SimEventDef types, EventDefId newtype, ModifierSource::Event variant, GameState.events field, events.json loading in sim_world, validation, Constants additions
2. **SE-02: Evaluation engine** — evaluate_events(), condition evaluation, weight computation (integer), candidate pool, weighted random selection, target resolution, integration in tick() as phase 4.5
3. **SE-03: Effect application + temporal modifiers** — All 6 EffectDef types applied, ActiveEffect tracking, expiry sweep, SimEventExpired emission, ModifierSet integration
4. **SE-04: Initial event content** — 6 events in events.json, constants values in constants.json
5. **SE-05: SSE + frontend integration** — SimEventFired/SimEventExpired Event variants, applyEvents handlers, Zod schemas, EventsFeed display with category styling, /content API event defs, ci_event_sync.sh update
6. **SE-06: Testing + determinism validation** — Unit tests, determinism regression scenario, integration test with real content, sim_bench events_test.json scenario

Dependencies: SE-01 → SE-02, SE-03, SE-04; SE-02 + SE-03 + SE-04 → SE-05; all → SE-06

## Open Questions

- **SpawnScanSite + replenish interaction**: Event-spawned sites count toward the replenish cap. Verify replenish_scan_sites() handles this correctly (it should — replenish caps at a target count).
- **Weight tuning**: Initial cooldown and weight values are educated guesses. Needs sim_bench tuning after engine is stable.
- **"All modules" targeting**: Micrometeorite Shower targets one random module in Phase 1. Add a `scope` field on EffectDef (single/all) in Phase 2 if "all modules" targeting is needed.
- **Parameterized items**: Supply Cache Discovery uses fixed item (repair_kits). Randomized item selection (additional RNG call) deferred to Phase 2.
- **Boolean condition trees**: Phase 1 uses flat condition lists (implicit AND). Extend to `{ all: [...] }` / `{ any: [...] }` trees via `#[serde(untagged)]` when needed.

## Future Phases (Not in Scope)

- **Phase 2: Choices + Chains** — Choice events (2-4 options, autopilot heuristic, timeout), chain events (follow-ups with delay/probability/context passing)
- **Phase 2: Storyteller** — Difficulty curve pacing, colony wealth/threat tracking, dramatic event weight adjustment
- **Phase 3: System Integration** — Exploration, crew, economic, environmental events from other game systems
