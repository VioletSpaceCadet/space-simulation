use sim_core::{CommandEnvelope, GameContent, GameState, PrincipalId};

#[allow(dead_code)] // Used starting in VIO-446/VIO-448
/// A decision-making agent that receives context and emits commands.
///
/// Agents are scoped — they see relevant state but only act within their
/// domain. The trait signature intentionally matches `AutopilotBehavior`
/// to allow incremental migration.
pub(crate) trait Agent: Send {
    /// Human-readable name for logging/debugging.
    fn name(&self) -> &'static str;

    /// Generate commands for this tick.
    fn generate(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}
