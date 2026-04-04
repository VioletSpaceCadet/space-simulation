pub(crate) mod concerns;
mod objectives;
#[cfg(test)]
mod tests;

use sim_core::{CommandEnvelope, GameContent, GameState, PrincipalId, StationId};

use super::Agent;
use super::DecisionRecord;
use concerns::{
    ComponentImport, CrewAssignment, CrewRecruitment, LabAssignment, MaterialExport,
    ModuleManagement, PropellantManagement, ShipFitting, SlagJettison,
};

/// Returns true if any enabled module has an unsatisfied crew requirement.
pub(in crate::agents) fn has_unsatisfied_crew_need(
    station: &sim_core::StationState,
    content: &GameContent,
) -> bool {
    station.core.modules.iter().any(|m| {
        m.enabled
            && !m.prev_crew_satisfied
            && content
                .module_defs
                .get(&m.def_id)
                .is_some_and(|d| !d.crew_requirement.is_empty())
    })
}

/// Context passed to each station concern on every tick.
pub(crate) struct StationContext<'a> {
    pub station_id: &'a StationId,
    pub state: &'a GameState,
    pub content: &'a GameContent,
    pub owner: &'a PrincipalId,
    pub next_id: &'a mut u64,
    pub trade_import_unlocked: bool,
    pub trade_export_unlocked: bool,
    pub decisions: Option<&'a mut Vec<DecisionRecord>>,
}

/// A composable station-level concern that generates commands.
///
/// Each concern is a separate struct. `StationAgent` holds a `Vec` of
/// concerns and runs them in order. Adding a new concern means creating
/// a new struct and adding it to `default_concerns()` — no changes to
/// `StationAgent` itself.
pub(crate) trait StationConcern: Send {
    /// Human-readable concern name for decision logging (VIO-468).
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    fn should_run(&self, ctx: &StationContext) -> bool;
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope>;
}

/// Per-station agent that composes ordered concerns.
///
/// Execution order is determined by `default_concerns()`:
/// modules → labs → crew → recruit → import → slag → exports →
/// propellant → ship fitting.
///
/// Created per `StationState`; removed when the station is removed from state.
pub(crate) struct StationAgent {
    pub(crate) station_id: StationId,
    concerns: Vec<Box<dyn StationConcern>>,
}

/// Default concern set for a station agent, in execution order.
fn default_concerns() -> Vec<Box<dyn StationConcern>> {
    vec![
        Box::new(ModuleManagement),
        Box::new(LabAssignment::default()),
        Box::new(CrewAssignment),
        Box::new(CrewRecruitment),
        Box::new(ComponentImport),
        Box::new(SlagJettison),
        Box::new(MaterialExport),
        Box::new(PropellantManagement),
        Box::new(ShipFitting),
    ]
}

impl StationAgent {
    pub(crate) fn new(station_id: StationId) -> Self {
        Self {
            station_id,
            concerns: default_concerns(),
        }
    }

    /// Run the propellant concern in isolation (used by lib.rs tests).
    #[cfg(test)]
    pub(crate) fn manage_propellant(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let mut ctx = StationContext {
            station_id: &self.station_id,
            state,
            content,
            owner,
            next_id,
            trade_import_unlocked: state
                .progression
                .trade_tier_unlocked(sim_core::TradeTier::BasicImport),
            trade_export_unlocked: state
                .progression
                .trade_tier_unlocked(sim_core::TradeTier::Export),
            decisions: None,
        };
        let mut concern = PropellantManagement;
        commands.extend(concern.generate(&mut ctx));
    }
}

impl Agent for StationAgent {
    fn name(&self) -> &'static str {
        "station_agent"
    }

    fn generate(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        mut decisions: Option<&mut Vec<DecisionRecord>>,
    ) -> Vec<CommandEnvelope> {
        if !state.stations.contains_key(&self.station_id) {
            return Vec::new();
        }

        let trade_import_unlocked = state
            .progression
            .trade_tier_unlocked(sim_core::TradeTier::BasicImport);
        let trade_export_unlocked = state
            .progression
            .trade_tier_unlocked(sim_core::TradeTier::Export);
        let mut commands = Vec::new();

        // Build context; disjoint field borrows allow &self.station_id + &mut self.concerns.
        let station_id = &self.station_id;
        for concern in &mut self.concerns {
            let mut ctx = StationContext {
                station_id,
                state,
                content,
                owner,
                next_id,
                trade_import_unlocked,
                trade_export_unlocked,
                #[allow(clippy::option_as_ref_deref)] // Need &mut Vec, not &mut [T]
                decisions: decisions.as_mut().map(|v| &mut **v),
            };
            if concern.should_run(&ctx) {
                commands.extend(concern.generate(&mut ctx));
            }
        }

        commands
    }
}
