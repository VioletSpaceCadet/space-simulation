pub(crate) mod ground_facility_agent;
pub(crate) mod ship_agent;
pub(crate) mod station_agent;

use sim_core::{CommandEnvelope, GameContent, GameState, PrincipalId};

/// Structured trace of an agent decision: what was chosen, what alternatives
/// existed, and scoring context. Flat layout for CSV/DuckDB queryability.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DecisionRecord {
    pub tick: u64,
    pub agent: String,
    pub concern: String,
    pub decision_type: String,
    pub chosen_id: String,
    pub chosen_score: f64,
    pub alt_1_id: String,
    pub alt_1_score: f64,
    pub alt_2_id: String,
    pub alt_2_score: f64,
    pub alt_3_id: String,
    pub alt_3_score: f64,
    pub context_json: String,
}

/// A decision-making agent that receives context and emits commands.
///
/// Agents are scoped — they see relevant state but only act within their
/// domain.
pub(crate) trait Agent: Send {
    /// Human-readable name for logging/debugging.
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    /// Generate commands for this tick. When `decisions` is `Some`, the agent
    /// should log key decisions (chosen + alternatives) for diagnostics.
    fn generate(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        decisions: Option<&mut Vec<DecisionRecord>>,
    ) -> Vec<CommandEnvelope>;
}
