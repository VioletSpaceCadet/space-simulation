//! Sim events engine — content-driven event system with composable effects.
//!
//! Types, evaluation engine, effect application, and validation for
//! content-driven sim events.

use std::collections::{BTreeMap, VecDeque};

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::modifiers::{ModifierOp, StatId};
use crate::{
    AlertSeverity, EventEnvelope, GameContent, GameState, ModuleInstanceId, ShipId, StationId,
    TradeItemSpec,
};

// ---------------------------------------------------------------------------
// EventDefId newtype
// ---------------------------------------------------------------------------

/// Unique identifier for a sim event definition (content-driven).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventDefId(pub String);

impl std::fmt::Display for EventDefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Rarity — resolved to base weight at load time
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl Rarity {
    /// Base weight for this rarity tier.
    pub fn base_weight(self) -> u32 {
        match self {
            Self::Common => 100,
            Self::Uncommon => 25,
            Self::Rare => 5,
            Self::Legendary => 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Conditions
// ---------------------------------------------------------------------------

/// A field on the game state that can be tested by a condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionField {
    Tick,
    StationCount,
    ShipCount,
    AvgModuleWear,
    Balance,
    TechsUnlockedCount,
}

/// Comparison operator for conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl CompareOp {
    /// Evaluate `lhs <op> rhs`.
    pub fn evaluate(self, lhs: f64, rhs: f64) -> bool {
        match self {
            Self::Gt => lhs > rhs,
            Self::Gte => lhs >= rhs,
            Self::Lt => lhs < rhs,
            Self::Lte => lhs <= rhs,
            Self::Eq => (lhs - rhs).abs() < 1e-6,
        }
    }
}

/// A single condition that must be met for an event to fire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub field: ConditionField,
    pub op: CompareOp,
    pub value: f64,
}

// ---------------------------------------------------------------------------
// Weight modifiers
// ---------------------------------------------------------------------------

/// Modifies an event's selection weight when a condition is met.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeightModifier {
    pub condition: Condition,
    /// Multiplier as integer percentage (e.g., 300 = 3x weight).
    pub weight_multiplier_pct: u32,
}

// ---------------------------------------------------------------------------
// Targeting
// ---------------------------------------------------------------------------

/// How to select a target entity when an event fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TargetingRule {
    Global,
    RandomStation,
    RandomShip,
    RandomModule { station: TargetStation },
    Zone { zone_id: Option<String> },
}

/// How to select a station for module targeting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetStation {
    Random,
    MostModules,
    HighestWear,
}

/// The resolved target after evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResolvedTarget {
    Global,
    Station {
        station_id: StationId,
    },
    Ship {
        ship_id: ShipId,
    },
    Module {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    Zone {
        zone_id: String,
    },
}

impl std::fmt::Display for ResolvedTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "the system"),
            Self::Station { station_id } => write!(f, "{station_id}"),
            Self::Ship { ship_id } => write!(f, "{ship_id}"),
            Self::Module {
                station_id,
                module_id,
            } => {
                write!(f, "{module_id} at {station_id}")
            }
            Self::Zone { zone_id } => write!(f, "{zone_id}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

/// A single effect that an event applies when it fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EffectDef {
    DamageModule {
        wear_amount: f32,
    },
    AddInventory {
        item: TradeItemSpec,
    },
    AddResearchData {
        data_kind: crate::DataKind,
        amount: f32,
    },
    SpawnScanSite {
        #[serde(default)]
        zone_override: Option<String>,
        #[serde(default)]
        template_override: Option<String>,
    },
    ApplyModifier {
        stat: StatId,
        op: ModifierOp,
        value: f64,
        duration_ticks: u64,
    },
    TriggerAlert {
        severity: AlertSeverity,
        message: String,
    },
}

/// An effect that was applied when an event fired (for history/SSE).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedEffect {
    pub effect: EffectDef,
    pub target: ResolvedTarget,
}

// ---------------------------------------------------------------------------
// SimEventDef — content-driven event definition
// ---------------------------------------------------------------------------

/// A sim event definition loaded from `content/events.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimEventDef {
    pub id: EventDefId,
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub rarity: Rarity,
    /// Override the rarity-derived base weight. Takes precedence over rarity.
    #[serde(default)]
    pub weight_override: Option<u32>,
    pub cooldown_ticks: u64,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub weight_modifiers: Vec<WeightModifier>,
    pub targeting: TargetingRule,
    pub effects: Vec<EffectDef>,
    /// Presentation template for the frontend (not used by `sim_core`).
    #[serde(default)]
    pub description_template: String,

    // -- Resolved at load time --
    /// Effective base weight (from `rarity` or `weight_override`). Set by content loading.
    #[serde(skip)]
    pub resolved_weight: u32,
}

impl SimEventDef {
    /// Resolve the base weight from `rarity` or `weight_override`.
    pub fn resolve_weight(&mut self) {
        self.resolved_weight = self
            .weight_override
            .unwrap_or_else(|| self.rarity.base_weight());
    }
}

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

/// A record of a fired event (stored in history ring buffer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FiredEvent {
    pub event_def_id: EventDefId,
    pub tick: u64,
    pub target: ResolvedTarget,
    pub effects_applied: Vec<AppliedEffect>,
}

/// A currently active temporal effect (modifier with expiry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveEffect {
    pub source_event_id: EventDefId,
    pub target: ResolvedTarget,
    pub expires_at_tick: u64,
}

/// Runtime state for the sim events system, stored on `GameState`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimEventState {
    /// Ring buffer of recently fired events.
    #[serde(default)]
    pub history: VecDeque<FiredEvent>,
    /// Per-event cooldowns: `event_def_id` → tick when cooldown expires.
    #[serde(default)]
    pub cooldowns: BTreeMap<EventDefId, u64>,
    /// Tick until which no event can fire (global cooldown).
    #[serde(default)]
    pub global_cooldown_until: u64,
    /// Currently active temporal effects (modifiers with expiry).
    #[serde(default)]
    pub active_effects: Vec<ActiveEffect>,
}

// ---------------------------------------------------------------------------
// Evaluation engine
// ---------------------------------------------------------------------------

/// Evaluate sim events for this tick. Selects at most one event to fire,
/// resolves its target, records it in history, and emits a `SimEventFired` event.
/// Applies effects, records the event in history, and emits `SimEventFired`.
pub fn evaluate_events(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    // Guard: events disabled
    if !content.constants.events_enabled {
        // Still sweep expired effects even when events are disabled
        sweep_expired_effects(state, events);
        return;
    }

    let tick = state.meta.tick;

    // Sweep expired temporal effects before evaluating new events
    sweep_expired_effects(state, events);

    // Check global cooldown
    if tick < state.events.global_cooldown_until {
        return;
    }

    // No event defs loaded
    if content.events.is_empty() {
        return;
    }

    // Build candidate pool: iterate event defs sorted by ID (content already sorted at load).
    // Filter by: all conditions pass, per-event cooldown not active, effective weight > 0.
    let mut candidates: Vec<(&SimEventDef, u64)> = Vec::new();

    // Sort event defs by ID for determinism
    let mut sorted_defs: Vec<&SimEventDef> = content.events.iter().collect();
    sorted_defs.sort_by(|a, b| a.id.cmp(&b.id));

    for def in &sorted_defs {
        // Check per-event cooldown
        if let Some(&cooldown_until) = state.events.cooldowns.get(&def.id) {
            if tick < cooldown_until {
                continue;
            }
        }

        // Check all conditions
        if !def.conditions.iter().all(|c| evaluate_condition(c, state)) {
            continue;
        }

        // Compute effective weight
        let weight = compute_effective_weight(def, state);
        if weight == 0 {
            continue;
        }

        candidates.push((def, weight));
    }

    if candidates.is_empty() {
        return;
    }

    // Weighted random selection using cumulative weights
    let total_weight: u64 = candidates.iter().map(|(_, w)| w).sum();
    let roll = rng.gen_range(0..total_weight);

    let mut cumulative = 0u64;
    let mut selected_idx = 0;
    for (index, (_, weight)) in candidates.iter().enumerate() {
        cumulative += weight;
        if roll < cumulative {
            selected_idx = index;
            break;
        }
    }

    let (selected_def, _) = candidates[selected_idx];

    // Resolve target
    let Some(target) = resolve_target(&selected_def.targeting, state, rng) else {
        return; // No valid target — skip this event
    };

    // Apply effects and record what was applied
    // Dual emission: SimEventFired = narrative, individual events = mechanical updates
    let effects_applied = apply_effects(
        &selected_def.id,
        &selected_def.effects,
        &target,
        state,
        content,
        rng,
        events,
    );

    let fired = FiredEvent {
        event_def_id: selected_def.id.clone(),
        tick,
        target: target.clone(),
        effects_applied: effects_applied.clone(),
    };

    // Update cooldowns
    state
        .events
        .cooldowns
        .insert(selected_def.id.clone(), tick + selected_def.cooldown_ticks);
    state.events.global_cooldown_until = tick + content.constants.event_global_cooldown_ticks;

    // Add to history ring buffer (respect capacity)
    state.events.history.push_back(fired);
    while state.events.history.len() > content.constants.event_history_capacity {
        state.events.history.pop_front();
    }

    // Emit SimEventFired event
    events.push(crate::emit(
        &mut state.counters,
        tick,
        crate::Event::SimEventFired {
            event_def_id: selected_def.id.clone(),
            target,
            effects_applied,
        },
    ));
}

/// Evaluate a single condition against the current game state.
pub fn evaluate_condition(condition: &Condition, state: &GameState) -> bool {
    let actual = extract_condition_value(&condition.field, state);
    condition.op.evaluate(actual, condition.value)
}

/// Extract the numeric value for a condition field from the game state.
fn extract_condition_value(field: &ConditionField, state: &GameState) -> f64 {
    match field {
        ConditionField::Tick => state.meta.tick as f64,
        ConditionField::StationCount => state.stations.len() as f64,
        ConditionField::ShipCount => state.ships.len() as f64,
        ConditionField::AvgModuleWear => {
            let mut total_wear = 0.0f64;
            let mut module_count = 0u64;
            for station in state.stations.values() {
                for module in &station.core.modules {
                    total_wear += f64::from(module.wear.wear);
                    module_count += 1;
                }
            }
            if module_count == 0 {
                0.0
            } else {
                total_wear / module_count as f64
            }
        }
        ConditionField::Balance => state.balance,
        ConditionField::TechsUnlockedCount => state.research.unlocked.len() as f64,
    }
}

/// Compute effective weight for an event def, applying weight modifiers.
/// Uses integer arithmetic: `base_weight * product(applicable_pct) / 100^n`.
pub fn compute_effective_weight(def: &SimEventDef, state: &GameState) -> u64 {
    let mut weight = u64::from(def.resolved_weight);

    for modifier in &def.weight_modifiers {
        if evaluate_condition(&modifier.condition, state) {
            weight = weight * u64::from(modifier.weight_multiplier_pct) / 100;
        }
    }

    weight
}

/// Resolve a targeting rule to a concrete target entity.
/// Returns `None` if no valid target exists (e.g., no stations when targeting a station).
pub fn resolve_target(
    rule: &TargetingRule,
    state: &GameState,
    rng: &mut impl Rng,
) -> Option<ResolvedTarget> {
    match rule {
        TargetingRule::Global => Some(ResolvedTarget::Global),

        TargetingRule::RandomStation => {
            let mut station_ids: Vec<&StationId> = state.stations.keys().collect();
            if station_ids.is_empty() {
                return None;
            }
            station_ids.sort();
            let index = rng.gen_range(0..station_ids.len());
            Some(ResolvedTarget::Station {
                station_id: station_ids[index].clone(),
            })
        }

        TargetingRule::RandomShip => {
            let mut ship_ids: Vec<&crate::ShipId> = state.ships.keys().collect();
            if ship_ids.is_empty() {
                return None;
            }
            ship_ids.sort();
            let index = rng.gen_range(0..ship_ids.len());
            Some(ResolvedTarget::Ship {
                ship_id: ship_ids[index].clone(),
            })
        }

        TargetingRule::RandomModule { station } => {
            // First select a station
            let selected_station_id = select_station(station, state, rng)?;
            let station_state = state.stations.get(&selected_station_id)?;

            if station_state.core.modules.is_empty() {
                return None;
            }

            // Pick a random module (sorted by ID for determinism)
            let mut module_ids: Vec<&ModuleInstanceId> =
                station_state.core.modules.iter().map(|m| &m.id).collect();
            module_ids.sort();
            let index = rng.gen_range(0..module_ids.len());
            Some(ResolvedTarget::Module {
                station_id: selected_station_id,
                module_id: module_ids[index].clone(),
            })
        }

        TargetingRule::Zone { zone_id } => {
            let zone = zone_id.clone().unwrap_or_else(|| "default".to_string());
            Some(ResolvedTarget::Zone { zone_id: zone })
        }
    }
}

/// Select a station based on the targeting strategy.
fn select_station(
    strategy: &TargetStation,
    state: &GameState,
    rng: &mut impl Rng,
) -> Option<StationId> {
    if state.stations.is_empty() {
        return None;
    }

    let mut station_ids: Vec<&StationId> = state.stations.keys().collect();
    station_ids.sort();

    match strategy {
        TargetStation::Random => {
            let index = rng.gen_range(0..station_ids.len());
            Some(station_ids[index].clone())
        }
        TargetStation::MostModules => {
            let best = station_ids
                .iter()
                .max_by_key(|id| state.stations[*id].core.modules.len())
                .expect("non-empty stations");
            Some((*best).clone())
        }
        TargetStation::HighestWear => {
            let best = station_ids
                .iter()
                .max_by(|a, b| {
                    let wear_a = avg_station_wear(&state.stations[*a]);
                    let wear_b = avg_station_wear(&state.stations[*b]);
                    wear_a
                        .partial_cmp(&wear_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .expect("non-empty stations");
            Some((*best).clone())
        }
    }
}

/// Compute average wear across all modules in a station.
fn avg_station_wear(station: &crate::StationState) -> f32 {
    if station.core.modules.is_empty() {
        return 0.0;
    }
    let total: f32 = station.core.modules.iter().map(|m| m.wear.wear).sum();
    total / station.core.modules.len() as f32
}

// ---------------------------------------------------------------------------
// Effect application
// ---------------------------------------------------------------------------

/// Apply all effects from a fired event. Returns the list of applied effects.
/// Each effect also emits its own mechanical event (dual emission contract).
fn apply_effects(
    event_def_id: &EventDefId,
    effect_defs: &[EffectDef],
    target: &ResolvedTarget,
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) -> Vec<AppliedEffect> {
    let mut applied = Vec::new();
    for effect in effect_defs {
        if let Some(result) =
            apply_single_effect(event_def_id, effect, target, state, content, rng, events)
        {
            applied.push(result);
        }
    }
    applied
}

fn apply_single_effect(
    event_def_id: &EventDefId,
    effect: &EffectDef,
    target: &ResolvedTarget,
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) -> Option<AppliedEffect> {
    let tick = state.meta.tick;
    match effect {
        EffectDef::DamageModule { wear_amount } => {
            apply_damage_module(*wear_amount, effect, target, state, rng, events)
        }
        EffectDef::AddInventory { item } => apply_add_inventory(item, effect, target, state, rng),
        EffectDef::AddResearchData { data_kind, amount } => {
            *state
                .research
                .data_pool
                .entry(data_kind.clone())
                .or_insert(0.0) += amount;
            events.push(crate::emit(
                &mut state.counters,
                tick,
                crate::Event::DataGenerated {
                    kind: data_kind.clone(),
                    amount: *amount,
                },
            ));
            Some(AppliedEffect {
                effect: effect.clone(),
                target: target.clone(),
            })
        }
        EffectDef::SpawnScanSite { .. } => {
            apply_spawn_scan_site(effect, target, state, content, rng, events)
        }
        EffectDef::ApplyModifier {
            stat,
            op,
            value,
            duration_ticks,
        } => {
            let modifier = crate::modifiers::Modifier {
                stat: *stat,
                op: *op,
                value: *value,
                source: crate::modifiers::ModifierSource::Event(event_def_id.0.clone()),
                condition: None,
            };
            apply_modifier(modifier, effect, target, state, *duration_ticks)
        }
        EffectDef::TriggerAlert { severity, message } => {
            let alert_id = format!("evt_{}", event_def_id.0);
            events.push(crate::emit(
                &mut state.counters,
                tick,
                crate::Event::AlertRaised {
                    alert_id,
                    severity: severity.clone(),
                    message: message.clone(),
                    suggested_action: String::new(),
                },
            ));
            Some(AppliedEffect {
                effect: effect.clone(),
                target: target.clone(),
            })
        }
    }
}

fn apply_damage_module(
    wear_amount: f32,
    effect: &EffectDef,
    target: &ResolvedTarget,
    state: &mut GameState,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) -> Option<AppliedEffect> {
    let (station_id, module_id) = resolve_module_target(target, state, rng)?;
    let station = state.stations.get_mut(&station_id)?;
    let module = station
        .core
        .modules
        .iter_mut()
        .find(|m| m.id == module_id)?;
    let wear_before = module.wear.wear;
    module.wear.wear = (module.wear.wear + wear_amount).min(1.0);
    events.push(crate::emit(
        &mut state.counters,
        state.meta.tick,
        crate::Event::WearAccumulated {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            wear_before,
            wear_after: module.wear.wear,
        },
    ));
    Some(AppliedEffect {
        effect: effect.clone(),
        target: ResolvedTarget::Module {
            station_id,
            module_id,
        },
    })
}

fn apply_add_inventory(
    item: &TradeItemSpec,
    effect: &EffectDef,
    target: &ResolvedTarget,
    state: &mut GameState,
    rng: &mut impl Rng,
) -> Option<AppliedEffect> {
    let station_id = resolve_station_target(target)?;
    let station = state.stations.get_mut(&station_id)?;
    let new_items = crate::trade::create_inventory_items(item, rng);
    station.core.inventory.extend(new_items);
    station.invalidate_volume_cache();
    Some(AppliedEffect {
        effect: effect.clone(),
        target: ResolvedTarget::Station { station_id },
    })
}

fn apply_spawn_scan_site(
    effect: &EffectDef,
    target: &ResolvedTarget,
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) -> Option<AppliedEffect> {
    let zone_bodies: Vec<&crate::OrbitalBodyDef> = content
        .solar_system
        .bodies
        .iter()
        .filter(|b| b.zone.is_some())
        .collect();
    if zone_bodies.is_empty() || content.asteroid_templates.is_empty() {
        return None;
    }
    let body = crate::pick_zone_weighted(&zone_bodies, rng);
    let zone_class = body.zone.as_ref().expect("zone body").resource_class;
    let template = crate::pick_template_biased(&content.asteroid_templates, zone_class, rng);
    let position = crate::random_position_in_zone(body, rng);
    let site_id = crate::SiteId(format!("site_evt_{}", crate::generate_uuid(rng)));
    state.scan_sites.push(crate::ScanSite {
        id: site_id.clone(),
        position: position.clone(),
        template_id: template.id.clone(),
    });
    events.push(crate::emit(
        &mut state.counters,
        state.meta.tick,
        crate::Event::ScanSiteSpawned {
            site_id,
            position,
            template_id: template.id.clone(),
        },
    ));
    Some(AppliedEffect {
        effect: effect.clone(),
        target: target.clone(),
    })
}

fn apply_modifier(
    modifier: crate::modifiers::Modifier,
    effect: &EffectDef,
    target: &ResolvedTarget,
    state: &mut GameState,
    duration_ticks: u64,
) -> Option<AppliedEffect> {
    let source_event_id = match &modifier.source {
        crate::modifiers::ModifierSource::Event(id) => EventDefId(id.clone()),
        _ => return None,
    };
    match target {
        ResolvedTarget::Global | ResolvedTarget::Zone { .. } => state.modifiers.add(modifier),
        ResolvedTarget::Station { station_id } | ResolvedTarget::Module { station_id, .. } => {
            state
                .stations
                .get_mut(station_id)?
                .core
                .modifiers
                .add(modifier);
        }
        ResolvedTarget::Ship { ship_id } => {
            state.ships.get_mut(ship_id)?.modifiers.add(modifier);
        }
    }
    state.events.active_effects.push(ActiveEffect {
        source_event_id,
        target: target.clone(),
        expires_at_tick: state.meta.tick + duration_ticks,
    });
    Some(AppliedEffect {
        effect: effect.clone(),
        target: target.clone(),
    })
}

/// Resolve a target to a (`station_id`, `module_id`) pair.
/// For Station targets, picks a random module within the station.
fn resolve_module_target(
    target: &ResolvedTarget,
    state: &GameState,
    rng: &mut impl Rng,
) -> Option<(StationId, ModuleInstanceId)> {
    match target {
        ResolvedTarget::Module {
            station_id,
            module_id,
        } => Some((station_id.clone(), module_id.clone())),
        ResolvedTarget::Station { station_id } => {
            let station = state.stations.get(station_id)?;
            if station.core.modules.is_empty() {
                return None;
            }
            let mut module_ids: Vec<&ModuleInstanceId> =
                station.core.modules.iter().map(|m| &m.id).collect();
            module_ids.sort();
            let index = rng.gen_range(0..module_ids.len());
            Some((station_id.clone(), module_ids[index].clone()))
        }
        _ => None,
    }
}

/// Resolve a target to a `station_id`.
fn resolve_station_target(target: &ResolvedTarget) -> Option<StationId> {
    match target {
        ResolvedTarget::Station { station_id } | ResolvedTarget::Module { station_id, .. } => {
            Some(station_id.clone())
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Expiry sweep
// ---------------------------------------------------------------------------

/// Remove expired temporal effects and clean up their modifiers.
fn sweep_expired_effects(state: &mut GameState, events: &mut Vec<EventEnvelope>) {
    if state.events.active_effects.is_empty() {
        return;
    }

    let tick = state.meta.tick;

    // Collect expired effects
    let (expired, remaining): (Vec<_>, Vec<_>) = state
        .events
        .active_effects
        .drain(..)
        .partition(|effect| effect.expires_at_tick <= tick);

    state.events.active_effects = remaining;

    // Remove modifiers and emit expiry events
    for expired_effect in &expired {
        let source =
            crate::modifiers::ModifierSource::Event(expired_effect.source_event_id.0.clone());

        match &expired_effect.target {
            ResolvedTarget::Global | ResolvedTarget::Zone { .. } => {
                state.modifiers.remove_by_source(&source);
            }
            ResolvedTarget::Station { station_id } | ResolvedTarget::Module { station_id, .. } => {
                if let Some(station) = state.stations.get_mut(station_id) {
                    station.core.modifiers.remove_by_source(&source);
                }
            }
            ResolvedTarget::Ship { ship_id } => {
                if let Some(ship) = state.ships.get_mut(ship_id) {
                    ship.modifiers.remove_by_source(&source);
                }
            }
        }

        events.push(crate::emit(
            &mut state.counters,
            tick,
            crate::Event::SimEventExpired {
                event_def_id: expired_effect.source_event_id.clone(),
            },
        ));
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate event definitions for authoring errors.
/// Panics on invalid definitions (consistent with existing content validation).
pub fn validate_event_defs(events: &[SimEventDef]) {
    let mut seen_ids = std::collections::HashSet::new();
    for event in events {
        // Unique IDs
        assert!(
            seen_ids.insert(&event.id),
            "duplicate event def id '{}'",
            event.id.0
        );

        // Cooldown must be positive
        assert!(
            event.cooldown_ticks > 0,
            "event '{}' must have cooldown_ticks > 0",
            event.id.0
        );

        // Weight modifiers must have non-zero multiplier
        for modifier in &event.weight_modifiers {
            assert!(
                modifier.weight_multiplier_pct > 0,
                "event '{}' has weight_modifier with 0 multiplier",
                event.id.0
            );
        }

        // Effect-targeting coherence
        for effect in &event.effects {
            validate_effect_values(effect, &event.id);
            validate_effect_targeting(effect, &event.targeting, &event.id);
        }
    }
}

/// Validate effect parameter ranges.
fn validate_effect_values(effect: &EffectDef, event_id: &EventDefId) {
    if let EffectDef::DamageModule { wear_amount } = effect {
        assert!(
            *wear_amount > 0.0 && *wear_amount <= 1.0,
            "event '{event_id}': DamageModule wear_amount must be in (0.0, 1.0], got {wear_amount}",
        );
    }
}

/// Validate that an effect is compatible with the event's targeting rule.
fn validate_effect_targeting(effect: &EffectDef, targeting: &TargetingRule, event_id: &EventDefId) {
    match effect {
        EffectDef::DamageModule { .. } => {
            assert!(
                matches!(
                    targeting,
                    TargetingRule::RandomStation | TargetingRule::RandomModule { .. }
                ),
                "event '{event_id}': DamageModule requires RandomStation or RandomModule targeting",
            );
        }
        EffectDef::AddInventory { .. } => {
            assert!(
                matches!(
                    targeting,
                    TargetingRule::RandomStation | TargetingRule::RandomModule { .. }
                ),
                "event '{event_id}': AddInventory requires RandomStation or RandomModule targeting",
            );
        }
        EffectDef::SpawnScanSite { .. } => {
            assert!(
                matches!(
                    targeting,
                    TargetingRule::Global | TargetingRule::Zone { .. }
                ),
                "event '{event_id}': SpawnScanSite requires Global or Zone targeting",
            );
        }
        // These effects work with any targeting — no validation needed
        EffectDef::AddResearchData { .. }
        | EffectDef::ApplyModifier { .. }
        | EffectDef::TriggerAlert { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event_def(id: &str, targeting: TargetingRule, effects: Vec<EffectDef>) -> SimEventDef {
        let mut def = SimEventDef {
            id: EventDefId(id.to_string()),
            name: id.to_string(),
            category: String::new(),
            tags: vec![],
            rarity: Rarity::Common,
            weight_override: None,
            cooldown_ticks: 100,
            conditions: vec![],
            weight_modifiers: vec![],
            targeting,
            effects,
            description_template: String::new(),
            resolved_weight: 0,
        };
        def.resolve_weight();
        def
    }

    #[test]
    fn rarity_base_weights() {
        assert_eq!(Rarity::Common.base_weight(), 100);
        assert_eq!(Rarity::Uncommon.base_weight(), 25);
        assert_eq!(Rarity::Rare.base_weight(), 5);
        assert_eq!(Rarity::Legendary.base_weight(), 1);
    }

    #[test]
    fn weight_override_takes_precedence() {
        let mut def = make_event_def("test", TargetingRule::Global, vec![]);
        def.rarity = Rarity::Common;
        def.weight_override = Some(42);
        def.resolve_weight();
        assert_eq!(def.resolved_weight, 42);
    }

    #[test]
    fn compare_op_evaluate() {
        assert!(CompareOp::Gt.evaluate(10.0, 5.0));
        assert!(!CompareOp::Gt.evaluate(5.0, 10.0));
        assert!(CompareOp::Gte.evaluate(5.0, 5.0));
        assert!(CompareOp::Lt.evaluate(3.0, 5.0));
        assert!(CompareOp::Lte.evaluate(5.0, 5.0));
        assert!(CompareOp::Eq.evaluate(5.0, 5.0));
        assert!(!CompareOp::Eq.evaluate(5.0, 5.1));
        // f32→f64 conversion introduces ~1.2e-8 error, which exceeds
        // f64::EPSILON (~2.2e-16) but is within the 1e-6 game tolerance.
        // This mirrors real usage: extract_condition_value casts f32 fields to f64.
        assert!(CompareOp::Eq.evaluate(0.3_f32 as f64, 0.3_f64));
    }

    #[test]
    fn serde_roundtrip_sim_event_state() {
        let state = SimEventState {
            history: VecDeque::from(vec![FiredEvent {
                event_def_id: EventDefId("evt_test".to_string()),
                tick: 42,
                target: ResolvedTarget::Station {
                    station_id: StationId("s1".to_string()),
                },
                effects_applied: vec![AppliedEffect {
                    effect: EffectDef::DamageModule { wear_amount: 0.3 },
                    target: ResolvedTarget::Module {
                        station_id: StationId("s1".to_string()),
                        module_id: ModuleInstanceId("m1".to_string()),
                    },
                }],
            }]),
            cooldowns: BTreeMap::from([(EventDefId("evt_test".to_string()), 500)]),
            global_cooldown_until: 300,
            active_effects: vec![ActiveEffect {
                source_event_id: EventDefId("evt_test".to_string()),
                target: ResolvedTarget::Global,
                expires_at_tick: 1000,
            }],
        };

        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: SimEventState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.history.len(), 1);
        assert_eq!(deserialized.cooldowns.len(), 1);
        assert_eq!(deserialized.global_cooldown_until, 300);
        assert_eq!(deserialized.active_effects.len(), 1);
    }

    #[test]
    fn serde_roundtrip_event_def() {
        let def = make_event_def(
            "evt_test",
            TargetingRule::RandomStation,
            vec![
                EffectDef::DamageModule { wear_amount: 0.3 },
                EffectDef::TriggerAlert {
                    severity: AlertSeverity::Warning,
                    message: "Test alert".to_string(),
                },
            ],
        );

        let json = serde_json::to_string(&def).expect("serialize");
        let deserialized: SimEventDef = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.id.0, "evt_test");
        assert_eq!(deserialized.effects.len(), 2);
    }

    #[test]
    fn serde_roundtrip_all_effect_variants() {
        let effects = vec![
            EffectDef::DamageModule { wear_amount: 0.5 },
            EffectDef::AddInventory {
                item: TradeItemSpec::Component {
                    component_id: crate::ComponentId("repair_kit".to_string()),
                    count: 5,
                },
            },
            EffectDef::AddResearchData {
                data_kind: crate::DataKind::new(crate::DataKind::SURVEY),
                amount: 10.0,
            },
            EffectDef::SpawnScanSite {
                zone_override: Some("inner_belt".to_string()),
                template_override: None,
            },
            EffectDef::ApplyModifier {
                stat: StatId::WearRate,
                op: ModifierOp::PctMultiplicative,
                value: 1.5,
                duration_ticks: 500,
            },
            EffectDef::TriggerAlert {
                severity: AlertSeverity::Critical,
                message: "Bad things!".to_string(),
            },
        ];

        for effect in &effects {
            let json = serde_json::to_string(effect).expect("serialize effect");
            let deserialized: EffectDef = serde_json::from_str(&json).expect("deserialize effect");
            assert_eq!(&deserialized, effect);
        }
    }

    #[test]
    fn serde_roundtrip_all_targeting_variants() {
        let variants = vec![
            TargetingRule::Global,
            TargetingRule::RandomStation,
            TargetingRule::RandomShip,
            TargetingRule::RandomModule {
                station: TargetStation::HighestWear,
            },
            TargetingRule::Zone {
                zone_id: Some("belt_1".to_string()),
            },
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).expect("serialize");
            let deserialized: TargetingRule = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&deserialized, variant);
        }
    }

    #[test]
    fn default_sim_event_state_is_empty() {
        let state = SimEventState::default();
        assert!(state.history.is_empty());
        assert!(state.cooldowns.is_empty());
        assert_eq!(state.global_cooldown_until, 0);
        assert!(state.active_effects.is_empty());
    }

    #[test]
    fn validate_accepts_valid_events() {
        let events = vec![
            make_event_def(
                "evt_a",
                TargetingRule::RandomStation,
                vec![EffectDef::DamageModule { wear_amount: 0.3 }],
            ),
            make_event_def(
                "evt_b",
                TargetingRule::Global,
                vec![EffectDef::SpawnScanSite {
                    zone_override: None,
                    template_override: None,
                }],
            ),
        ];
        validate_event_defs(&events); // Should not panic
    }

    #[test]
    #[should_panic(expected = "duplicate event def id")]
    fn validate_rejects_duplicate_ids() {
        let events = vec![
            make_event_def("evt_dup", TargetingRule::Global, vec![]),
            make_event_def("evt_dup", TargetingRule::Global, vec![]),
        ];
        validate_event_defs(&events);
    }

    #[test]
    #[should_panic(expected = "cooldown_ticks > 0")]
    fn validate_rejects_zero_cooldown() {
        let mut def = make_event_def("evt_bad", TargetingRule::Global, vec![]);
        def.cooldown_ticks = 0;
        validate_event_defs(&[def]);
    }

    #[test]
    #[should_panic(expected = "weight_modifier with 0 multiplier")]
    fn validate_rejects_zero_weight_multiplier() {
        let mut def = make_event_def("evt_bad", TargetingRule::Global, vec![]);
        def.weight_modifiers.push(WeightModifier {
            condition: Condition {
                field: ConditionField::Tick,
                op: CompareOp::Gte,
                value: 100.0,
            },
            weight_multiplier_pct: 0,
        });
        validate_event_defs(&[def]);
    }

    #[test]
    #[should_panic(expected = "DamageModule requires")]
    fn validate_rejects_damage_module_with_global_targeting() {
        let events = vec![make_event_def(
            "evt_bad",
            TargetingRule::Global,
            vec![EffectDef::DamageModule { wear_amount: 0.3 }],
        )];
        validate_event_defs(&events);
    }

    #[test]
    #[should_panic(expected = "AddInventory requires")]
    fn validate_rejects_add_inventory_with_global_targeting() {
        let events = vec![make_event_def(
            "evt_bad",
            TargetingRule::Global,
            vec![EffectDef::AddInventory {
                item: TradeItemSpec::Component {
                    component_id: crate::ComponentId("repair_kit".to_string()),
                    count: 1,
                },
            }],
        )];
        validate_event_defs(&events);
    }

    #[test]
    #[should_panic(expected = "SpawnScanSite requires")]
    fn validate_rejects_spawn_scan_site_with_station_targeting() {
        let events = vec![make_event_def(
            "evt_bad",
            TargetingRule::RandomStation,
            vec![EffectDef::SpawnScanSite {
                zone_override: None,
                template_override: None,
            }],
        )];
        validate_event_defs(&events);
    }

    #[test]
    fn resolved_target_display() {
        assert_eq!(ResolvedTarget::Global.to_string(), "the system");
        assert_eq!(
            ResolvedTarget::Station {
                station_id: StationId("s1".to_string())
            }
            .to_string(),
            "s1"
        );
    }

    #[test]
    fn backward_compat_game_state_without_events() {
        // Old save format without the `events` field should deserialize to default
        let json = r#"{
            "meta": {"tick": 100, "seed": 42, "schema_version": 1, "content_version": "test"},
            "scan_sites": [],
            "asteroids": {},
            "ships": {},
            "stations": {},
            "research": {"unlocked": [], "data_pool": {}, "evidence": {}, "action_counts": {}},
            "counters": {"next_event_id": 0, "next_command_id": 0, "next_asteroid_id": 0, "next_lot_id": 0, "next_module_instance_id": 0}
        }"#;

        let state: crate::GameState = serde_json::from_str(json).expect("deserialize old format");
        assert!(state.events.history.is_empty());
        assert!(state.events.cooldowns.is_empty());
        assert_eq!(state.events.global_cooldown_until, 0);
        assert!(state.events.active_effects.is_empty());
    }

    #[test]
    fn modifier_source_event_roundtrip() {
        use crate::modifiers::{Modifier, ModifierOp, ModifierSet, ModifierSource, StatId};

        let mut set = ModifierSet::new();
        set.add(Modifier::pct_mult(
            StatId::WearRate,
            1.5,
            ModifierSource::Event("evt_solar_flare".to_string()),
        ));

        let json = serde_json::to_string(&set).expect("serialize");
        let deserialized: ModifierSet = serde_json::from_str(&json).expect("deserialize");

        // Modifier should resolve correctly
        let result = deserialized.resolve(StatId::WearRate, 1.0);
        assert!((result - 1.5).abs() < 1e-10);

        // Remove by source should work
        let mut set2 = deserialized;
        set2.remove_by_source(&ModifierSource::Event("evt_solar_flare".to_string()));
        assert!(set2.is_empty());
    }

    // -----------------------------------------------------------------------
    // Evaluation engine tests
    // -----------------------------------------------------------------------

    use crate::test_fixtures::{base_content, base_state, make_rng};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn make_events_content() -> crate::GameContent {
        let mut content = base_content();
        content.constants.events_enabled = true;
        content.constants.event_global_cooldown_ticks = 10;
        content.constants.event_history_capacity = 5;
        content.events = vec![
            {
                let mut def = make_event_def(
                    "evt_a",
                    TargetingRule::Global,
                    vec![EffectDef::TriggerAlert {
                        severity: AlertSeverity::Warning,
                        message: "Event A".to_string(),
                    }],
                );
                def.cooldown_ticks = 50;
                def
            },
            {
                let mut def = make_event_def(
                    "evt_b",
                    TargetingRule::Global,
                    vec![EffectDef::TriggerAlert {
                        severity: AlertSeverity::Warning,
                        message: "Event B".to_string(),
                    }],
                );
                def.cooldown_ticks = 50;
                def.rarity = Rarity::Rare;
                def.resolve_weight();
                def
            },
        ];
        content
    }

    #[test]
    fn evaluate_condition_tick() {
        let content = base_content();
        let mut state = base_state(&content);
        state.meta.tick = 500;

        let condition = Condition {
            field: ConditionField::Tick,
            op: CompareOp::Gte,
            value: 100.0,
        };
        assert!(evaluate_condition(&condition, &state));

        let condition2 = Condition {
            field: ConditionField::Tick,
            op: CompareOp::Gte,
            value: 1000.0,
        };
        assert!(!evaluate_condition(&condition2, &state));
    }

    #[test]
    fn evaluate_condition_station_count() {
        let content = base_content();
        let state = base_state(&content);

        let condition = Condition {
            field: ConditionField::StationCount,
            op: CompareOp::Gte,
            value: 1.0,
        };
        assert!(evaluate_condition(&condition, &state));
    }

    #[test]
    fn evaluate_condition_ship_count() {
        let content = base_content();
        let state = base_state(&content);

        let condition = Condition {
            field: ConditionField::ShipCount,
            op: CompareOp::Gte,
            value: 1.0,
        };
        assert!(evaluate_condition(&condition, &state));
    }

    #[test]
    fn evaluate_condition_balance() {
        let content = base_content();
        let mut state = base_state(&content);
        state.balance = 500_000.0;

        let condition = Condition {
            field: ConditionField::Balance,
            op: CompareOp::Lt,
            value: 1_000_000.0,
        };
        assert!(evaluate_condition(&condition, &state));
    }

    #[test]
    fn evaluate_condition_techs_unlocked() {
        let content = base_content();
        let state = base_state(&content);

        let condition = Condition {
            field: ConditionField::TechsUnlockedCount,
            op: CompareOp::Eq,
            value: 0.0,
        };
        assert!(evaluate_condition(&condition, &state));
    }

    #[test]
    fn compute_weight_no_modifiers() {
        let content = base_content();
        let state = base_state(&content);

        let def = make_event_def("evt", TargetingRule::Global, vec![]);
        assert_eq!(compute_effective_weight(&def, &state), 100); // Common = 100
    }

    #[test]
    fn compute_weight_with_active_modifier() {
        let content = base_content();
        let mut state = base_state(&content);
        state.meta.tick = 500;

        let mut def = make_event_def("evt", TargetingRule::Global, vec![]);
        def.weight_modifiers.push(WeightModifier {
            condition: Condition {
                field: ConditionField::Tick,
                op: CompareOp::Gte,
                value: 100.0,
            },
            weight_multiplier_pct: 300, // 3x
        });
        // base 100 * 300 / 100 = 300
        assert_eq!(compute_effective_weight(&def, &state), 300);
    }

    #[test]
    fn compute_weight_with_inactive_modifier() {
        let content = base_content();
        let mut state = base_state(&content);
        state.meta.tick = 50;

        let mut def = make_event_def("evt", TargetingRule::Global, vec![]);
        def.weight_modifiers.push(WeightModifier {
            condition: Condition {
                field: ConditionField::Tick,
                op: CompareOp::Gte,
                value: 100.0,
            },
            weight_multiplier_pct: 300,
        });
        // Condition not met, base weight stays 100
        assert_eq!(compute_effective_weight(&def, &state), 100);
    }

    #[test]
    fn events_disabled_produces_nothing() {
        let mut content = make_events_content();
        content.constants.events_enabled = false;
        let mut state = base_state(&content);
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);
        assert!(events.is_empty());
    }

    #[test]
    fn global_cooldown_blocks_events() {
        let content = make_events_content();
        let mut state = base_state(&content);
        state.events.global_cooldown_until = 999; // Far future
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);
        assert!(events.is_empty());
    }

    #[test]
    fn per_event_cooldown_blocks_specific_event() {
        let content = make_events_content();
        let mut state = base_state(&content);
        // Put both events on cooldown
        state
            .events
            .cooldowns
            .insert(EventDefId("evt_a".to_string()), 999);
        state
            .events
            .cooldowns
            .insert(EventDefId("evt_b".to_string()), 999);
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);
        assert!(events.is_empty());
    }

    #[test]
    fn condition_filters_events() {
        let mut content = make_events_content();
        // Add condition that tick >= 1000 for both events
        for event in &mut content.events {
            event.conditions.push(Condition {
                field: ConditionField::Tick,
                op: CompareOp::Gte,
                value: 1000.0,
            });
        }

        let mut state = base_state(&content);
        state.meta.tick = 500; // Doesn't meet condition
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);
        assert!(events.is_empty());
    }

    #[test]
    fn event_fires_and_records_history() {
        let content = make_events_content();
        let mut state = base_state(&content);
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);

        // Should have fired one sim event (dual emission: effect events + SimEventFired)
        assert!(
            events.len() >= 2,
            "Expected at least 2 events (effect + SimEventFired), got {}",
            events.len()
        );
        assert_eq!(state.events.history.len(), 1);

        // Last event should be SimEventFired
        let last = &events.last().expect("events").event;
        assert!(
            matches!(last, crate::Event::SimEventFired { .. }),
            "Last event should be SimEventFired"
        );

        // Should have set cooldowns
        assert!(state.events.global_cooldown_until > 0);
        let fired = &state.events.history[0];
        assert!(state.events.cooldowns.contains_key(&fired.event_def_id));
    }

    #[test]
    fn history_ring_buffer_respects_capacity() {
        let content = make_events_content();
        let mut state = base_state(&content);
        let mut rng = make_rng();

        // Fire events 10 times (capacity is 5)
        for tick_num in 0..10 {
            state.meta.tick = tick_num * 100; // Space out past cooldowns
            state.events.global_cooldown_until = 0;
            state.events.cooldowns.clear();
            let mut events = Vec::new();
            evaluate_events(&mut state, &content, &mut rng, &mut events);
        }

        assert!(state.events.history.len() <= 5);
    }

    #[test]
    fn determinism_same_seed_same_result() {
        let content = make_events_content();

        // Run 1
        let mut state1 = base_state(&content);
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut events1 = Vec::new();
        evaluate_events(&mut state1, &content, &mut rng1, &mut events1);

        // Run 2
        let mut state2 = base_state(&content);
        let mut rng2 = ChaCha8Rng::seed_from_u64(42);
        let mut events2 = Vec::new();
        evaluate_events(&mut state2, &content, &mut rng2, &mut events2);

        assert_eq!(events1.len(), events2.len());
        assert_eq!(state1.events.history, state2.events.history);
    }

    #[test]
    fn weighted_selection_favors_common() {
        let content = make_events_content();
        // evt_a = Common (100), evt_b = Rare (5)
        let mut count_a = 0;
        let mut count_b = 0;

        for seed in 0..200 {
            let mut state = base_state(&content);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut events = Vec::new();
            evaluate_events(&mut state, &content, &mut rng, &mut events);

            if let Some(fired) = state.events.history.front() {
                if fired.event_def_id.0 == "evt_a" {
                    count_a += 1;
                } else {
                    count_b += 1;
                }
            }
        }

        // evt_a should fire much more often (100/105 ≈ 95%)
        assert!(
            count_a > count_b * 5,
            "Expected evt_a ({count_a}) to fire much more than evt_b ({count_b})"
        );
    }

    #[test]
    fn resolve_target_random_station() {
        let content = base_content();
        let state = base_state(&content);
        let mut rng = make_rng();

        let target = resolve_target(&TargetingRule::RandomStation, &state, &mut rng);
        assert!(matches!(target, Some(ResolvedTarget::Station { .. })));
    }

    #[test]
    fn resolve_target_random_ship() {
        let content = base_content();
        let state = base_state(&content);
        let mut rng = make_rng();

        let target = resolve_target(&TargetingRule::RandomShip, &state, &mut rng);
        assert!(matches!(target, Some(ResolvedTarget::Ship { .. })));
    }

    #[test]
    fn resolve_target_returns_none_for_empty_entities() {
        let content = base_content();
        let mut state = base_state(&content);
        state.stations.clear();
        let mut rng = make_rng();

        let target = resolve_target(&TargetingRule::RandomStation, &state, &mut rng);
        assert!(target.is_none());
    }

    #[test]
    fn resolve_target_global() {
        let content = base_content();
        let state = base_state(&content);
        let mut rng = make_rng();

        let target = resolve_target(&TargetingRule::Global, &state, &mut rng);
        assert_eq!(target, Some(ResolvedTarget::Global));
    }

    #[test]
    fn no_event_fires_with_no_valid_target() {
        let mut content = make_events_content();
        // Change targeting to RandomStation
        for event in &mut content.events {
            event.targeting = TargetingRule::RandomStation;
        }
        let mut state = base_state(&content);
        state.stations.clear(); // No stations
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);
        assert!(events.is_empty());
    }

    // -----------------------------------------------------------------------
    // Effect application tests
    // -----------------------------------------------------------------------

    fn make_damage_event_content() -> crate::GameContent {
        let mut content = base_content();
        content.constants.events_enabled = true;
        content.constants.event_global_cooldown_ticks = 10;
        content.events = vec![{
            let mut def = make_event_def(
                "evt_damage",
                TargetingRule::RandomStation,
                vec![EffectDef::DamageModule { wear_amount: 0.2 }],
            );
            def.cooldown_ticks = 50;
            def
        }];
        content
    }

    /// Add a test module to the first station.
    fn add_test_module(state: &mut crate::GameState) {
        let station = state.stations.values_mut().next().expect("station");
        station.core.modules.push(crate::ModuleState {
            id: ModuleInstanceId("mod_test".to_string()),
            def_id: "module_basic_iron_refinery".to_string(),
            enabled: true,
            kind_state: crate::ModuleKindState::Processor(crate::ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
            wear: crate::WearState { wear: 0.1 },
            thermal: None,
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            slot_index: None,
        });
    }

    #[test]
    fn effect_damage_module_increases_wear() {
        let content = make_damage_event_content();
        let mut state = base_state(&content);
        add_test_module(&mut state);
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);

        // Check wear increased from 0.1
        let station = state.stations.values().next().expect("station");
        let module = &station.core.modules[0];
        assert!(
            module.wear.wear > 0.1,
            "wear should have increased from 0.1 to {}",
            module.wear.wear
        );

        // Should have WearAccumulated + SimEventFired events
        assert!(events
            .iter()
            .any(|e| matches!(&e.event, crate::Event::WearAccumulated { .. })));
    }

    #[test]
    fn effect_add_research_data() {
        let mut content = base_content();
        content.constants.events_enabled = true;
        content.constants.event_global_cooldown_ticks = 10;
        content.events = vec![{
            let mut def = make_event_def(
                "evt_data",
                TargetingRule::Global,
                vec![EffectDef::AddResearchData {
                    data_kind: crate::DataKind::new(crate::DataKind::SURVEY),
                    amount: 50.0,
                }],
            );
            def.cooldown_ticks = 50;
            def
        }];

        let mut state = base_state(&content);
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);

        let survey_data = state
            .research
            .data_pool
            .get(&crate::DataKind::new(crate::DataKind::SURVEY))
            .copied()
            .unwrap_or(0.0);
        assert!(
            survey_data >= 50.0,
            "Expected >=50 survey data, got {survey_data}"
        );

        assert!(events
            .iter()
            .any(|e| matches!(&e.event, crate::Event::DataGenerated { .. })));
    }

    #[test]
    fn effect_trigger_alert() {
        let content = make_events_content(); // Uses TriggerAlert effects
        let mut state = base_state(&content);
        let mut rng = make_rng();
        let mut events = Vec::new();

        evaluate_events(&mut state, &content, &mut rng, &mut events);

        assert!(events
            .iter()
            .any(|e| matches!(&e.event, crate::Event::AlertRaised { .. })));
    }

    #[test]
    fn effect_apply_modifier_lifecycle() {
        let mut content = base_content();
        content.constants.events_enabled = true;
        content.constants.event_global_cooldown_ticks = 10;
        content.events = vec![{
            let mut def = make_event_def(
                "evt_modifier",
                TargetingRule::Global,
                vec![EffectDef::ApplyModifier {
                    stat: StatId::WearRate,
                    op: ModifierOp::PctMultiplicative,
                    value: 1.5,
                    duration_ticks: 100,
                }],
            );
            def.cooldown_ticks = 50;
            def
        }];

        let mut state = base_state(&content);
        let mut rng = make_rng();
        let mut events = Vec::new();

        // Fire event at tick 0
        evaluate_events(&mut state, &content, &mut rng, &mut events);

        // Modifier should be active
        assert_eq!(state.events.active_effects.len(), 1);
        let wear_rate = state.modifiers.resolve(StatId::WearRate, 1.0);
        assert!(
            (wear_rate - 1.5).abs() < 1e-10,
            "Expected 1.5x wear rate, got {wear_rate}"
        );

        // Advance past expiry and disable events to test sweep only
        state.meta.tick = 200;
        let mut content2 = content.clone();
        content2.constants.events_enabled = false;
        let mut events2 = Vec::new();
        evaluate_events(&mut state, &content2, &mut rng, &mut events2);

        // Modifier should be expired and removed
        assert!(
            state.events.active_effects.is_empty(),
            "active effects should be empty after expiry"
        );
        let wear_rate_after = state.modifiers.resolve(StatId::WearRate, 1.0);
        assert!(
            (wear_rate_after - 1.0).abs() < 1e-10,
            "Expected 1.0x wear rate after expiry, got {wear_rate_after}"
        );

        // Should have SimEventExpired event
        assert!(events2
            .iter()
            .any(|e| matches!(&e.event, crate::Event::SimEventExpired { .. })));
    }

    #[test]
    fn effect_add_inventory_components() {
        let mut content = base_content();
        content.constants.events_enabled = true;
        content.constants.event_global_cooldown_ticks = 10;
        content.events = vec![{
            let mut def = make_event_def(
                "evt_supply",
                TargetingRule::RandomStation,
                vec![EffectDef::AddInventory {
                    item: TradeItemSpec::Component {
                        component_id: crate::ComponentId("repair_kit".to_string()),
                        count: 5,
                    },
                }],
            );
            def.cooldown_ticks = 50;
            def
        }];

        let mut state = base_state(&content);
        let mut rng = make_rng();
        let mut events = Vec::new();

        // Count initial repair kits
        let station = state.stations.values().next().expect("station");
        let initial_kits: u32 = station
            .core
            .inventory
            .iter()
            .filter_map(|item| match item {
                crate::InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "repair_kit" => Some(*count),
                _ => None,
            })
            .sum();

        evaluate_events(&mut state, &content, &mut rng, &mut events);

        let station = state.stations.values().next().expect("station");
        let final_kits: u32 = station
            .core
            .inventory
            .iter()
            .filter_map(|item| match item {
                crate::InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "repair_kit" => Some(*count),
                _ => None,
            })
            .sum();

        assert_eq!(
            final_kits,
            initial_kits + 5,
            "Expected {initial_kits} + 5 = {} kits, got {final_kits}",
            initial_kits + 5
        );
    }

    #[test]
    fn sweep_only_runs_when_active_effects_exist() {
        let content = make_events_content();
        let mut state = base_state(&content);
        let mut events = Vec::new();

        // No active effects — sweep should be a no-op
        sweep_expired_effects(&mut state, &mut events);
        assert!(events.is_empty());
    }
}
