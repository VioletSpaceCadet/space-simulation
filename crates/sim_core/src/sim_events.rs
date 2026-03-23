//! Sim events engine — content-driven event system with composable effects.
//!
//! Types for event definitions, conditions, targeting, effects, and runtime state.
//! Evaluation and effect application are in separate tickets (SE-02, SE-03).

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::modifiers::{ModifierOp, StatId};
use crate::{AlertSeverity, ModuleInstanceId, ShipId, StationId, TradeItemSpec};

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
            Self::Eq => (lhs - rhs).abs() < f64::EPSILON,
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
        domain: crate::ResearchDomain,
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
                domain: crate::ResearchDomain::Survey,
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
}
