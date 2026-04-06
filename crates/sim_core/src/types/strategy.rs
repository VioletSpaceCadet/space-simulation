//! Strategic configuration types for the autopilot.
//!
//! `StrategyConfig` is the tunable knob set that shapes high-level autopilot
//! behavior: what to prioritize, how large a fleet to maintain, and operational
//! thresholds for propellant, exports, and budgets. It lives on `GameState` so
//! it can be changed at runtime (via `SetStrategyConfig` — VIO-483) and on
//! `GameContent.default_strategy` so it can be seeded from `content/strategy.json`.
//!
//! This module defines types only — no behavior. Consumers are wired in VIO-480
//! (rule interpreter) and VIO-481 (station agent consumption).

use crate::GamePhase;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// StrategyMode
// ---------------------------------------------------------------------------

/// High-level strategic stance. Each mode yields a set of multipliers applied
/// to the base `PriorityWeights`, producing the effective weights used by the
/// rule interpreter.
///
/// `StrategyMode` is an engine-mechanic enum (not content-driven) because it
/// maps to hand-tuned multiplier tables, not loose content categories.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyMode {
    /// Neutral stance. All multipliers are 1.0.
    #[default]
    Balanced,
    /// Expansion stance. Boosts mining, export, and fleet growth.
    Expand,
    /// Consolidation stance. Boosts research, maintenance, and propellant.
    Consolidate,
}

impl StrategyMode {
    /// Multiplier table for this mode. Effective weight = base * multiplier,
    /// clamped to \[0.0, 1.0\] by `StrategyConfig::effective_priorities`.
    #[must_use]
    pub fn multipliers(self) -> PriorityWeights {
        match self {
            Self::Balanced => PriorityWeights {
                mining: 1.0,
                survey: 1.0,
                deep_scan: 1.0,
                research: 1.0,
                maintenance: 1.0,
                export: 1.0,
                propellant: 1.0,
                fleet_expansion: 1.0,
            },
            Self::Expand => PriorityWeights {
                mining: 1.25,
                survey: 1.15,
                deep_scan: 1.0,
                research: 0.85,
                maintenance: 0.9,
                export: 1.2,
                propellant: 1.0,
                fleet_expansion: 1.3,
            },
            Self::Consolidate => PriorityWeights {
                mining: 0.9,
                survey: 0.85,
                deep_scan: 1.0,
                research: 1.25,
                maintenance: 1.2,
                export: 0.9,
                propellant: 1.15,
                fleet_expansion: 0.75,
            },
        }
    }

    /// Default strategy mode for a given game phase (VIO-607).
    /// Used by phase-driven auto-switching when no manual override is set.
    #[must_use]
    pub fn for_phase(phase: GamePhase) -> Self {
        match phase {
            GamePhase::Startup | GamePhase::Orbital => Self::Balanced,
            GamePhase::Industrial | GamePhase::DeepSpace => Self::Expand,
            GamePhase::Expansion => Self::Expand,
        }
    }
}

// ---------------------------------------------------------------------------
// PriorityWeights
// ---------------------------------------------------------------------------

/// Named-struct representation of the 8 autopilot concern weights. Using a
/// named struct (instead of a `BTreeMap<String, f32>`) gives compile-time
/// safety, no hash overhead, and a stable field order for the optimizer
/// interface (`to_vec` / `from_vec`).
///
/// The `priorities` field on `StrategyConfig` is in \[0.0, 1.0\]; the rule
/// interpreter (VIO-480) multiplies these by state urgency to produce
/// per-concern scores. `PriorityWeights` is also reused as the return type of
/// `StrategyMode::multipliers()`, where values are unbounded positive floats
/// (e.g. 1.3 for "boost by 30%"). `StrategyConfig::effective_priorities`
/// multiplies the two and clamps the result back to \[0.0, 1.0\].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PriorityWeights {
    pub mining: f32,
    pub survey: f32,
    pub deep_scan: f32,
    pub research: f32,
    pub maintenance: f32,
    pub export: f32,
    pub propellant: f32,
    pub fleet_expansion: f32,
}

impl Default for PriorityWeights {
    fn default() -> Self {
        Self {
            mining: 0.7,
            survey: 0.6,
            deep_scan: 0.5,
            research: 0.6,
            maintenance: 0.8,
            export: 0.5,
            propellant: 0.9,
            fleet_expansion: 0.5,
        }
    }
}

impl PriorityWeights {
    /// Number of fields in `PriorityWeights`. Kept in sync with `to_vec` /
    /// `from_vec` and enforced by a test.
    pub const LEN: usize = 8;

    /// Serialize weights to a fixed-order `Vec<f32>` for optimizer interfaces.
    /// Field order: `mining`, `survey`, `deep_scan`, `research`, `maintenance`,
    /// `export`, `propellant`, `fleet_expansion`.
    #[must_use]
    pub fn to_vec(&self) -> Vec<f32> {
        vec![
            self.mining,
            self.survey,
            self.deep_scan,
            self.research,
            self.maintenance,
            self.export,
            self.propellant,
            self.fleet_expansion,
        ]
    }

    /// Deserialize weights from a fixed-order slice. Returns `None` if the
    /// slice length does not match `PriorityWeights::LEN`.
    #[must_use]
    pub fn from_vec(values: &[f32]) -> Option<Self> {
        if values.len() != Self::LEN {
            return None;
        }
        Some(Self {
            mining: values[0],
            survey: values[1],
            deep_scan: values[2],
            research: values[3],
            maintenance: values[4],
            export: values[5],
            propellant: values[6],
            fleet_expansion: values[7],
        })
    }

    /// Element-wise multiply two weight structs (used to apply mode multipliers).
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        Self {
            mining: self.mining * other.mining,
            survey: self.survey * other.survey,
            deep_scan: self.deep_scan * other.deep_scan,
            research: self.research * other.research,
            maintenance: self.maintenance * other.maintenance,
            export: self.export * other.export,
            propellant: self.propellant * other.propellant,
            fleet_expansion: self.fleet_expansion * other.fleet_expansion,
        }
    }

    /// Mutable references to every field in a fixed canonical order matching
    /// `to_vec` / `from_vec` / `LEN`. Used by code that needs to apply
    /// element-wise in-place updates (hysteresis bonus, temporal bias) without
    /// repeating the 8-line field list at every call site.
    ///
    /// Field order: `mining`, `survey`, `deep_scan`, `research`, `maintenance`,
    /// `export`, `propellant`, `fleet_expansion`.
    pub fn fields_mut(&mut self) -> [&mut f32; Self::LEN] {
        [
            &mut self.mining,
            &mut self.survey,
            &mut self.deep_scan,
            &mut self.research,
            &mut self.maintenance,
            &mut self.export,
            &mut self.propellant,
            &mut self.fleet_expansion,
        ]
    }

    /// Clamp every field to \[0.0, 1.0\] in place. Used after applying mode
    /// multipliers so the rule interpreter can treat weights as probabilities.
    /// NaN values are replaced with 0.0 (safer default for an urgency weight
    /// than propagating NaN into downstream arithmetic and float comparisons).
    pub fn clamp_unit(&mut self) {
        self.mining = sanitize_unit(self.mining);
        self.survey = sanitize_unit(self.survey);
        self.deep_scan = sanitize_unit(self.deep_scan);
        self.research = sanitize_unit(self.research);
        self.maintenance = sanitize_unit(self.maintenance);
        self.export = sanitize_unit(self.export);
        self.propellant = sanitize_unit(self.propellant);
        self.fleet_expansion = sanitize_unit(self.fleet_expansion);
    }
}

/// Clamp a float to \[0.0, 1.0\] and replace NaN with 0.0.
fn sanitize_unit(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// StrategyConfig
// ---------------------------------------------------------------------------

/// Strategic configuration. Lives on `GameState` (seeded from
/// `GameContent.default_strategy`) and can be replaced at runtime via
/// `Command::SetStrategyConfig` (VIO-483).
///
/// **Version history:**
/// * `strategy-v1` (VIO-479) — mode, priorities, fleet target, 9 thresholds
///   mirroring `Constants.autopilot_*`.
/// * `strategy-v2` (VIO-605) — superset of the P0 `AutopilotConfig` behavioral
///   parameters. Adds `refuel_max_pct`, `shipyard_component_count`,
///   `power_deficit_threshold_kw`, and `crew_hire_projection_minutes` so
///   `sim_bench` and the optimizer can tune the full surface.
///
/// The behavioral fields mirror `Constants.autopilot_*` / `AutopilotConfig`
/// with identical defaults — no consumers read from `StrategyConfig` yet, so
/// behavior is unchanged. Consumer switching is tracked separately and lands
/// alongside VIO-480/481 when the rule interpreter and station agents are
/// wired up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrategyConfig {
    /// Schema version for migration / audit.
    pub version: String,

    /// High-level stance (multiplicative modifier on `priorities`).
    pub mode: StrategyMode,

    /// Base concern weights in \[0.0, 1.0\]. Apply `mode.multipliers()` to get
    /// effective weights — see `effective_priorities`.
    pub priorities: PriorityWeights,

    /// Target active fleet size. The expansion concern becomes urgent below
    /// this number.
    pub fleet_size_target: u32,

    // -- Operational thresholds (defaults match Constants.autopilot_* / AutopilotConfig) --
    /// H2O inventory (kg) below which the autopilot prioritizes volatile-rich
    /// mining. Default 500.0. Mirrors `Constants.autopilot_volatile_threshold_kg`.
    pub volatile_threshold_kg: f32,
    /// LH2 inventory threshold (kg) for propellant pipeline management.
    /// Default 5000.0. Mirrors `Constants.autopilot_lh2_threshold_kg`.
    pub lh2_threshold_kg: f32,
    /// Multiplier on `lh2_threshold_kg` above which electrolysis is disabled
    /// to save power. Default 2.0. Mirrors `Constants.autopilot_lh2_abundant_multiplier`.
    pub lh2_abundant_multiplier: f32,
    /// Default refinery processing threshold (kg) for newly installed
    /// processor modules. Default 2000.0. Mirrors
    /// `Constants.autopilot_refinery_threshold_kg`.
    pub refinery_threshold_kg: f32,
    /// Cargo fraction at which slag is jettisoned. Default 0.75. Mirrors
    /// `Constants.autopilot_slag_jettison_pct`.
    pub slag_jettison_pct: f32,
    /// Max kg per material export command per tick. Default 500.0. Mirrors
    /// `Constants.autopilot_export_batch_size_kg`.
    pub export_batch_size_kg: f32,
    /// Minimum revenue threshold — skip exports yielding less than this.
    /// Default 1000.0. Mirrors `Constants.autopilot_export_min_revenue`.
    pub export_min_revenue: f64,
    /// Max fraction of balance the autopilot will spend on a single import.
    /// Default 0.05. Mirrors `Constants.autopilot_budget_cap_fraction`.
    pub budget_cap_fraction: f64,
    /// Propellant fraction below which ships opportunistically refuel at a
    /// station. Default 0.8. Mirrors `Constants.autopilot_refuel_threshold_pct`.
    pub refuel_threshold_pct: f32,

    /// Manual mode override. When `Some`, auto-switching based on game phase
    /// is disabled and this mode is used instead. Set to `None` to re-enable
    /// phase-driven auto-switching (VIO-607).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_override: Option<StrategyMode>,

    // -- strategy-v2 additions (VIO-605): remaining P0 AutopilotConfig behavioral params --
    /// Propellant fraction at or above which refueling is considered complete.
    /// Default 0.99. Mirrors `AutopilotConfig::refuel_max_pct` /
    /// `Constants.autopilot_refuel_max_pct`.
    pub refuel_max_pct: f32,
    /// Default component import count for shipyard recipes when no recipe
    /// match is found. Default 4. Mirrors `AutopilotConfig::shipyard_component_count` /
    /// `Constants.autopilot_shipyard_component_count`.
    pub shipyard_component_count: u32,
    /// Power deficit (kW) below which module shedding is not triggered.
    /// Default 0.01. Mirrors `AutopilotConfig::power_deficit_threshold_kw`.
    pub power_deficit_threshold_kw: f32,
    /// Forward-looking salary projection window (game-minutes) for crew
    /// hiring decisions. Default `30 * 24 * 60 = 43_200` (30 days).
    /// Mirrors `AutopilotConfig::crew_hire_projection_minutes`.
    pub crew_hire_projection_minutes: u64,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            version: "strategy-v2".to_string(),
            mode: StrategyMode::Balanced,
            priorities: PriorityWeights::default(),
            fleet_size_target: 3,
            mode_override: None,
            volatile_threshold_kg: 500.0,
            lh2_threshold_kg: 5000.0,
            lh2_abundant_multiplier: 2.0,
            refinery_threshold_kg: 2000.0,
            slag_jettison_pct: 0.75,
            export_batch_size_kg: 500.0,
            export_min_revenue: 1_000.0,
            budget_cap_fraction: 0.05,
            refuel_threshold_pct: 0.8,
            refuel_max_pct: 0.99,
            shipyard_component_count: 4,
            power_deficit_threshold_kw: 0.01,
            crew_hire_projection_minutes: 30 * 24 * 60,
        }
    }
}

impl StrategyConfig {
    /// Compute effective priority weights: `priorities * mode.multipliers()`
    /// clamped to \[0.0, 1.0\].
    #[must_use]
    pub fn effective_priorities(&self) -> PriorityWeights {
        let mut effective = self.priorities.mul(&self.mode.multipliers());
        effective.clamp_unit();
        effective
    }
}

// ---------------------------------------------------------------------------
// ConcernPriorities
// ---------------------------------------------------------------------------

/// Output of the strategy rule interpreter — final per-concern urgency scores
/// in \[0.0, 1.0\] that guide station and ship agent decisions. Computed by
/// `sim_control::AutopilotController::evaluate_strategy` from
/// `StrategyConfig` + `GameState` using `config_weight * state_urgency`
/// multiplied by the mode multipliers, plus hysteresis and temporal bias.
///
/// Shape is identical to `PriorityWeights` — the type alias keeps the
/// optimizer interface (`to_vec` / `from_vec`) and semantics consistent
/// across the configuration-side (user weights) and the output side
/// (interpreter-computed scores).
pub type ConcernPriorities = PriorityWeights;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_strategy_mode_is_balanced() {
        let config = StrategyConfig::default();
        assert_eq!(config.mode, StrategyMode::Balanced);
    }

    #[test]
    fn balanced_multipliers_are_all_one() {
        let mults = StrategyMode::Balanced.multipliers();
        assert_eq!(mults.mining, 1.0);
        assert_eq!(mults.survey, 1.0);
        assert_eq!(mults.deep_scan, 1.0);
        assert_eq!(mults.research, 1.0);
        assert_eq!(mults.maintenance, 1.0);
        assert_eq!(mults.export, 1.0);
        assert_eq!(mults.propellant, 1.0);
        assert_eq!(mults.fleet_expansion, 1.0);
    }

    #[test]
    fn expand_mode_boosts_mining_and_fleet() {
        let mults = StrategyMode::Expand.multipliers();
        assert!(mults.mining > 1.0);
        assert!(mults.fleet_expansion > 1.0);
        assert!(mults.research < 1.0);
    }

    #[test]
    fn consolidate_mode_boosts_research_and_maintenance() {
        let mults = StrategyMode::Consolidate.multipliers();
        assert!(mults.research > 1.0);
        assert!(mults.maintenance > 1.0);
        assert!(mults.fleet_expansion < 1.0);
    }

    #[test]
    fn priority_weights_to_vec_from_vec_roundtrip() {
        let weights = PriorityWeights {
            mining: 0.1,
            survey: 0.2,
            deep_scan: 0.3,
            research: 0.4,
            maintenance: 0.5,
            export: 0.6,
            propellant: 0.7,
            fleet_expansion: 0.8,
        };
        let vec = weights.to_vec();
        assert_eq!(vec.len(), PriorityWeights::LEN);
        let restored = PriorityWeights::from_vec(&vec).unwrap();
        assert_eq!(restored, weights);
    }

    #[test]
    fn from_vec_rejects_wrong_length() {
        assert!(PriorityWeights::from_vec(&[0.1, 0.2, 0.3]).is_none());
        assert!(PriorityWeights::from_vec(&[0.0; 9]).is_none());
    }

    #[test]
    fn effective_priorities_clamped_to_unit_range() {
        let config = StrategyConfig {
            mode: StrategyMode::Expand,
            priorities: PriorityWeights {
                mining: 0.9, // 0.9 * 1.25 = 1.125 -> clamped to 1.0
                survey: 0.0,
                deep_scan: 0.5,
                research: 0.5,
                maintenance: 0.5,
                export: 0.5,
                propellant: 0.5,
                fleet_expansion: 0.9, // 0.9 * 1.3 = 1.17 -> clamped to 1.0
            },
            ..StrategyConfig::default()
        };
        let effective = config.effective_priorities();
        assert_eq!(effective.mining, 1.0);
        assert_eq!(effective.fleet_expansion, 1.0);
        assert!((effective.deep_scan - 0.5).abs() < 1e-6);
    }

    #[test]
    fn strategy_config_default_matches_constants_thresholds() {
        let config = StrategyConfig::default();
        // strategy-v1 threshold fields — must match Constants.autopilot_* defaults.
        assert!((config.volatile_threshold_kg - 500.0).abs() < 1e-6);
        assert!((config.lh2_threshold_kg - 5000.0).abs() < 1e-6);
        assert!((config.lh2_abundant_multiplier - 2.0).abs() < 1e-6);
        assert!((config.refinery_threshold_kg - 2000.0).abs() < 1e-6);
        assert!((config.slag_jettison_pct - 0.75).abs() < 1e-6);
        assert!((config.export_batch_size_kg - 500.0).abs() < 1e-6);
        assert!((config.export_min_revenue - 1_000.0).abs() < 1e-9);
        assert!((config.budget_cap_fraction - 0.05).abs() < 1e-9);
        assert!((config.refuel_threshold_pct - 0.8).abs() < 1e-6);
        // strategy-v2 additions (VIO-605) — must match AutopilotConfig defaults
        // so consumer switching in a follow-up ticket is a no-op.
        assert!((config.refuel_max_pct - 0.99).abs() < 1e-6);
        assert_eq!(config.shipyard_component_count, 4);
        assert!((config.power_deficit_threshold_kw - 0.01).abs() < 1e-6);
        assert_eq!(config.crew_hire_projection_minutes, 30 * 24 * 60);
    }

    #[test]
    fn strategy_config_default_version_is_v2() {
        assert_eq!(StrategyConfig::default().version, "strategy-v2");
    }

    #[test]
    fn strategy_v1_json_deserializes_as_v2_with_defaults() {
        // Backward-compat: an old strategy-v1 JSON (no v2 fields) must still
        // parse. Missing v2 fields fall back to `StrategyConfig::default()`
        // so existing saves continue to work.
        let v1_json = r#"{
            "version": "strategy-v1",
            "mode": "Balanced",
            "fleet_size_target": 3
        }"#;
        let parsed: StrategyConfig = serde_json::from_str(v1_json).unwrap();
        assert_eq!(parsed.version, "strategy-v1");
        assert_eq!(parsed.fleet_size_target, 3);
        // v2 fields take defaults.
        assert!((parsed.refuel_max_pct - 0.99).abs() < 1e-6);
        assert_eq!(parsed.shipyard_component_count, 4);
        assert!((parsed.power_deficit_threshold_kw - 0.01).abs() < 1e-6);
        assert_eq!(parsed.crew_hire_projection_minutes, 30 * 24 * 60);
    }

    #[test]
    fn strategy_config_roundtrips_through_json() {
        let config = StrategyConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: StrategyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn strategy_config_deserializes_from_empty_object() {
        // Backward-compat: missing field on GameState must deserialize to default.
        let parsed: StrategyConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, StrategyConfig::default());
    }

    #[test]
    fn strategy_config_deserializes_partial_override() {
        // Optimizer / API will often send only a subset of fields.
        let json = r#"{"mode":"Expand","fleet_size_target":8}"#;
        let parsed: StrategyConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.mode, StrategyMode::Expand);
        assert_eq!(parsed.fleet_size_target, 8);
        assert_eq!(parsed.priorities, PriorityWeights::default());
    }

    #[test]
    fn priorities_deserialize_partial_subset() {
        // Nested partial deserialization: only some weight fields specified,
        // the rest must fall back to `PriorityWeights::default()`.
        let json = r#"{"priorities":{"mining":0.1,"research":0.2}}"#;
        let parsed: StrategyConfig = serde_json::from_str(json).unwrap();
        assert!((parsed.priorities.mining - 0.1).abs() < 1e-6);
        assert!((parsed.priorities.research - 0.2).abs() < 1e-6);
        // Unspecified fields preserve the struct defaults.
        assert!((parsed.priorities.maintenance - 0.8).abs() < 1e-6);
        assert!((parsed.priorities.propellant - 0.9).abs() < 1e-6);
    }

    #[test]
    fn fields_mut_matches_to_vec_order() {
        // Pin the mutable-reference iteration order to the fixed canonical
        // order. If this drifts, hysteresis/temporal-bias code that uses
        // `fields_mut` will silently apply updates to the wrong concerns.
        let mut weights = PriorityWeights {
            mining: 1.0,
            survey: 2.0,
            deep_scan: 3.0,
            research: 4.0,
            maintenance: 5.0,
            export: 6.0,
            propellant: 7.0,
            fleet_expansion: 8.0,
        };
        let values: Vec<f32> = weights.fields_mut().iter().map(|f| **f).collect();
        assert_eq!(values, weights.to_vec());
    }

    #[test]
    fn priority_weights_len_matches_to_vec_length() {
        // Pin the public `LEN` constant to the actual serialized width so a
        // future field addition cannot drift the two out of sync silently.
        let vec = PriorityWeights::default().to_vec();
        assert_eq!(vec.len(), PriorityWeights::LEN);
        assert_eq!(PriorityWeights::LEN, 8);
    }

    #[test]
    fn clamp_unit_sanitizes_nan_to_zero() {
        let mut weights = PriorityWeights {
            mining: f32::NAN,
            survey: 0.5,
            deep_scan: f32::INFINITY,
            research: f32::NEG_INFINITY,
            maintenance: 2.0,
            export: -0.5,
            propellant: 1.0,
            fleet_expansion: 0.25,
        };
        weights.clamp_unit();
        // NaN becomes 0.0, not NaN-propagating downstream.
        assert_eq!(weights.mining, 0.0);
        assert_eq!(weights.survey, 0.5);
        // Infinities clamp to the bounds.
        assert_eq!(weights.deep_scan, 1.0);
        assert_eq!(weights.research, 0.0);
        assert_eq!(weights.maintenance, 1.0);
        assert_eq!(weights.export, 0.0);
        assert_eq!(weights.propellant, 1.0);
        assert!((weights.fleet_expansion - 0.25).abs() < 1e-6);
    }
}
