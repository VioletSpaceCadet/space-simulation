use sim_core::{Command, CommandEnvelope, InventoryItem};

use crate::behaviors::make_cmd;

use super::super::{GroundFacilityConcern, GroundFacilityContext};

/// Install modules from inventory into the facility.
pub(in crate::agents) struct ModuleInstall;

impl GroundFacilityConcern for ModuleInstall {
    fn name(&self) -> &'static str {
        "module_install"
    }
    fn should_run(&self, _ctx: &GroundFacilityContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut GroundFacilityContext) -> Vec<CommandEnvelope> {
        let Some(facility) = ctx.state.ground_facilities.get(ctx.facility_id) else {
            return Vec::new();
        };

        let mut commands = Vec::new();
        for item in &facility.core.inventory {
            if let InventoryItem::Module { item_id, .. } = item {
                commands.push(make_cmd(
                    ctx.owner,
                    ctx.state.meta.tick,
                    ctx.next_id,
                    Command::InstallModule {
                        facility_id: ctx.facility_id.clone().into(),
                        module_item_id: item_id.clone(),
                    },
                ));
            }
        }
        commands
    }
}
