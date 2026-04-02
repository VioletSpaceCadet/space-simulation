//! Progression system types: milestones, phases, trade tiers, grants.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Game phase (progression)
// ---------------------------------------------------------------------------

/// Descriptive game phase derived from milestone completion.
/// Named `GamePhase` to avoid conflict with `Phase` (material phase: Solid/Liquid).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
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
