//! Progression system types: milestones, phases, trade tiers, grants, progression state.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Game phase (progression)
// ---------------------------------------------------------------------------

/// Descriptive game phase derived from milestone completion.
/// Named `GamePhase` to avoid conflict with `Phase` (material phase: Solid/Liquid).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[repr(u32)]
pub enum GamePhase {
    #[default]
    Startup,
    Orbital,
    Industrial,
    Expansion,
    DeepSpace,
}

impl std::fmt::Display for GamePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Startup => write!(f, "Startup"),
            Self::Orbital => write!(f, "Orbital"),
            Self::Industrial => write!(f, "Industrial"),
            Self::Expansion => write!(f, "Expansion"),
            Self::DeepSpace => write!(f, "Deep Space"),
        }
    }
}

// ---------------------------------------------------------------------------
// Trade tier
// ---------------------------------------------------------------------------

/// Trade capability tier, unlocked by milestone rewards.
/// Ordered: `None` < `BasicImport` < `Export` < `Full`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum TradeTier {
    #[default]
    None,
    BasicImport,
    Export,
    Full,
}

// ---------------------------------------------------------------------------
// Milestone definitions (content-driven)
// ---------------------------------------------------------------------------

/// A milestone definition loaded from `content/milestones.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub conditions: Vec<MilestoneCondition>,
    pub rewards: MilestoneReward,
    /// If set, advancing to this phase when the milestone completes.
    #[serde(default)]
    pub phase_advance: Option<GamePhase>,
}

/// A condition that must be met for a milestone to complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MilestoneCondition {
    /// A `MetricsSnapshot` field must be >= threshold.
    #[serde(rename = "metric_above")]
    MetricAbove { field: String, threshold: f64 },
    /// A game state counter must be >= threshold.
    #[serde(rename = "counter_above")]
    CounterAbove { counter: String, threshold: f64 },
    /// A prerequisite milestone must be completed.
    #[serde(rename = "milestone_completed")]
    MilestoneCompleted { milestone_id: String },
}

/// Rewards applied when a milestone completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneReward {
    /// Grant amount added to balance.
    #[serde(default)]
    pub grant_amount: f64,
    /// Reputation points awarded.
    #[serde(default)]
    pub reputation: f64,
    /// Trade tier to unlock (only upgrades, never downgrades).
    #[serde(default)]
    pub unlock_trade_tier: Option<TradeTier>,
    /// Zone IDs to unlock for scan site replenishment.
    #[serde(default)]
    pub unlock_zone_ids: Vec<String>,
    /// Module def IDs to make available.
    #[serde(default)]
    pub unlock_module_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Progression state (runtime)
// ---------------------------------------------------------------------------

/// Runtime progression state stored in `GameState`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProgressionState {
    /// IDs of completed milestones.
    #[serde(default)]
    pub completed_milestones: BTreeSet<String>,
    /// Current game phase (descriptive label derived from milestones).
    #[serde(default)]
    pub phase: GamePhase,
    /// Record of all grants received.
    #[serde(default)]
    pub grant_history: Vec<GrantRecord>,
    /// Cumulative reputation score.
    #[serde(default)]
    pub reputation: f64,
    /// Current trade capability tier.
    #[serde(default)]
    pub trade_tier: TradeTier,
    /// Zone IDs unlocked by milestone rewards.
    #[serde(default)]
    pub unlocked_zone_ids: BTreeSet<String>,
    /// Module def IDs unlocked by milestone rewards.
    #[serde(default)]
    pub unlocked_module_ids: BTreeSet<String>,
}

impl ProgressionState {
    /// Check if a specific milestone has been completed.
    pub fn is_milestone_completed(&self, milestone_id: &str) -> bool {
        self.completed_milestones.contains(milestone_id)
    }

    /// Check if the current trade tier is at least the given tier.
    pub fn trade_tier_unlocked(&self, required: TradeTier) -> bool {
        self.trade_tier >= required
    }
}

/// Record of a grant payment received from a milestone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantRecord {
    pub milestone_id: String,
    pub amount: f64,
    pub tick: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_progression_state() {
        let state = ProgressionState::default();
        assert!(state.completed_milestones.is_empty());
        assert_eq!(state.phase, GamePhase::Startup);
        assert_eq!(state.trade_tier, TradeTier::None);
        assert_eq!(state.reputation, 0.0);
        assert!(state.grant_history.is_empty());
    }

    #[test]
    fn is_milestone_completed() {
        let mut state = ProgressionState::default();
        assert!(!state.is_milestone_completed("first_survey"));
        state
            .completed_milestones
            .insert("first_survey".to_string());
        assert!(state.is_milestone_completed("first_survey"));
        assert!(!state.is_milestone_completed("first_ore"));
    }

    #[test]
    fn trade_tier_unlocked() {
        let mut state = ProgressionState::default();
        assert!(state.trade_tier_unlocked(TradeTier::None));
        assert!(!state.trade_tier_unlocked(TradeTier::BasicImport));

        state.trade_tier = TradeTier::Export;
        assert!(state.trade_tier_unlocked(TradeTier::None));
        assert!(state.trade_tier_unlocked(TradeTier::BasicImport));
        assert!(state.trade_tier_unlocked(TradeTier::Export));
        assert!(!state.trade_tier_unlocked(TradeTier::Full));
    }

    #[test]
    fn game_phase_ordering() {
        assert!(GamePhase::Startup < GamePhase::Orbital);
        assert!(GamePhase::Orbital < GamePhase::Industrial);
        assert!(GamePhase::Industrial < GamePhase::Expansion);
        assert!(GamePhase::Expansion < GamePhase::DeepSpace);
    }
}
