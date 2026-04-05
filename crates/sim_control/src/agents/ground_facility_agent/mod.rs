mod concerns;
#[cfg(test)]
mod tests;

use sim_core::{CommandEnvelope, GameContent, GameState, GroundFacilityId, PrincipalId};

use super::Agent;
use super::DecisionRecord;
use concerns::{ComponentPurchase, LaunchExecution, ModuleInstall, SensorBudget, SensorPurchase};

/// Context passed to each ground facility concern on every tick.
#[allow(dead_code)]
pub(crate) struct GroundFacilityContext<'a> {
    pub facility_id: &'a GroundFacilityId,
    pub state: &'a GameState,
    pub content: &'a GameContent,
    pub owner: &'a PrincipalId,
    pub next_id: &'a mut u64,
    pub decisions: Option<&'a mut Vec<DecisionRecord>>,
}

/// A composable ground-facility-level concern that generates commands.
pub(crate) trait GroundFacilityConcern: Send {
    /// Human-readable concern name for decision logging.
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    fn should_run(&self, ctx: &GroundFacilityContext) -> bool;
    fn generate(&mut self, ctx: &mut GroundFacilityContext) -> Vec<CommandEnvelope>;
}

/// Per-ground-facility agent that composes ordered concerns.
///
/// Execution order: install modules → purchase sensors → manage sensor budget →
/// purchase components → execute launches.
///
/// Sensor data flows into the global research pool, so station labs
/// automatically benefit from ground sensor output.
pub(crate) struct GroundFacilityAgent {
    pub(crate) facility_id: GroundFacilityId,
    concerns: Vec<Box<dyn GroundFacilityConcern>>,
}

fn default_concerns() -> Vec<Box<dyn GroundFacilityConcern>> {
    vec![
        Box::new(ModuleInstall),
        Box::new(SensorPurchase),
        Box::new(SensorBudget),
        Box::new(ComponentPurchase),
        Box::new(LaunchExecution),
    ]
}

impl GroundFacilityAgent {
    pub(crate) fn new(facility_id: GroundFacilityId) -> Self {
        Self {
            facility_id,
            concerns: default_concerns(),
        }
    }
}

impl Agent for GroundFacilityAgent {
    fn name(&self) -> &'static str {
        "ground_facility_agent"
    }

    fn generate(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        mut decisions: Option<&mut Vec<DecisionRecord>>,
    ) -> Vec<CommandEnvelope> {
        if !state.ground_facilities.contains_key(&self.facility_id) {
            return Vec::new();
        }

        let mut commands = Vec::new();
        let facility_id = &self.facility_id;
        for concern in &mut self.concerns {
            let mut ctx = GroundFacilityContext {
                facility_id,
                state,
                content,
                owner,
                next_id,
                #[allow(clippy::option_as_ref_deref)]
                decisions: decisions.as_mut().map(|v| &mut **v),
            };
            if concern.should_run(&ctx) {
                commands.extend(concern.generate(&mut ctx));
            }
        }

        commands
    }
}
