use std::collections::HashMap;

use sim_core::{
    CommandEnvelope, GameContent, GameState, PrincipalId, ResearchDomain, StationId, TechId,
};

use super::Agent;

/// Per-station agent that consolidates all station-level behaviors into
/// ordered sub-concern methods.
///
/// Execution order within `generate()` matches the legacy `default_behaviors()`
/// ordering (AD5 from plan). Each sub-concern is a method, not a separate trait
/// object — keeps it simple and avoids dynamic dispatch overhead.
///
/// Created per `StationState`; removed when the station is removed from state.
#[allow(dead_code)] // Sub-concerns filled in VIO-450
pub(crate) struct StationAgent {
    pub(crate) station_id: StationId,
    pub(crate) lab_cache: LabAssignmentCache,
}

/// Per-station cache for lab assignment decisions.
///
/// Mirrors the cache from `LabAssignment` behavior but is scoped to a single
/// station (AD6 from plan). Rebuilt when the set of unlocked techs changes.
#[derive(Default)]
#[allow(dead_code)] // Used starting in VIO-450
pub(crate) struct LabAssignmentCache {
    /// domain → eligible tech IDs (prereqs met, not yet unlocked, needs this domain).
    pub(crate) cached_eligible: HashMap<ResearchDomain, Vec<TechId>>,
    /// Number of unlocked techs when cache was last built.
    pub(crate) last_unlocked_count: usize,
    /// Whether the cache has been initialized at all.
    pub(crate) initialized: bool,
}

#[allow(dead_code)] // Sub-concerns filled in VIO-450
#[allow(clippy::unused_self)] // Stubs take &mut self for future implementation
impl StationAgent {
    pub(crate) fn new(station_id: StationId) -> Self {
        Self {
            station_id,
            lab_cache: LabAssignmentCache::default(),
        }
    }

    // --- Sub-concern stubs (filled in VIO-450) ---
    // Execution order matches default_behaviors() for determinism (AD5).

    /// 1. Install/uninstall/enable/disable station modules.
    fn manage_modules(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 2. Assign research labs to eligible techs.
    fn assign_labs(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 3. Assign available crew to understaffed modules by priority.
    fn assign_crew(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 4. Recruit crew when roles are needed.
    fn recruit_crew(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 5. Import thruster components for shipyard.
    fn import_components(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 6. Jettison slag when above threshold.
    fn jettison_slag(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 7. Export surplus materials for revenue.
    fn export_materials(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 8. Manage propellant pipeline (enable/disable electrolysis).
    fn manage_propellant(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 9. Fit idle ships at this station with available modules.
    fn fit_ships(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
    }

    /// 10. Assign ship objectives to idle ships (absorbed from bridge in VIO-451).
    fn assign_ship_objectives(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_id: &mut u64,
        _commands: &mut Vec<CommandEnvelope>,
    ) {
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
        _owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        // Verify this station still exists
        if !state.stations.contains_key(&self.station_id) {
            return Vec::new();
        }

        let mut commands = Vec::new();

        // Sub-concerns execute in order matching default_behaviors() (AD5)
        self.manage_modules(state, content, next_id, &mut commands);
        self.assign_labs(state, content, next_id, &mut commands);
        self.assign_crew(state, content, next_id, &mut commands);
        self.recruit_crew(state, content, next_id, &mut commands);
        self.import_components(state, content, next_id, &mut commands);
        self.jettison_slag(state, content, next_id, &mut commands);
        self.export_materials(state, content, next_id, &mut commands);
        self.manage_propellant(state, content, next_id, &mut commands);
        self.fit_ships(state, content, next_id, &mut commands);
        self.assign_ship_objectives(state, content, next_id, &mut commands);

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::test_fixtures::{base_content, base_state};

    #[test]
    fn new_agent_has_empty_lab_cache() {
        let agent = StationAgent::new(StationId("test_station".to_string()));
        assert_eq!(agent.station_id, StationId("test_station".to_string()));
        assert!(!agent.lab_cache.initialized);
        assert_eq!(agent.lab_cache.last_unlocked_count, 0);
        assert!(agent.lab_cache.cached_eligible.is_empty());
    }

    #[test]
    fn stubs_produce_no_commands() {
        let content = base_content();
        let state = base_state(&content);
        let owner = PrincipalId("principal_autopilot".to_string());
        let station_id = state.stations.keys().next().unwrap().clone();
        let mut agent = StationAgent::new(station_id);
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
    }

    #[test]
    fn missing_station_produces_no_commands() {
        let content = base_content();
        let state = base_state(&content);
        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = StationAgent::new(StationId("nonexistent".to_string()));
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
    }
}
