use sim_core::{Command, CommandEnvelope};

use crate::behaviors::{make_cmd, total_element_inventory};

use super::super::{StationConcern, StationContext};

/// 8. Toggle propellant modules based on global LH2 levels (hysteresis).
pub(crate) struct PropellantManagement;

impl StationConcern for PropellantManagement {
    fn name(&self) -> &'static str {
        "propellant_management"
    }
    fn should_run(&self, _ctx: &StationContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        let propellant_role = &ctx.content.autopilot.propellant_role;
        let support_role = &ctx.content.autopilot.propellant_support_role;

        if !station.has_role(propellant_role) {
            return Vec::new();
        }

        let propellant_kg =
            total_element_inventory(ctx.state, &ctx.content.autopilot.propellant_element);
        let threshold = ctx.content.autopilot.lh2_threshold_kg;
        let mut commands = Vec::new();

        if propellant_kg > threshold * ctx.content.autopilot.lh2_abundant_multiplier {
            for &module_idx in station.modules_with_role(propellant_role) {
                let module = &station.core.modules[module_idx];
                if module.enabled && module.wear.wear < 1.0 {
                    commands.push(make_cmd(
                        ctx.owner,
                        ctx.state.meta.tick,
                        ctx.next_id,
                        Command::SetModuleEnabled {
                            station_id: ctx.station_id.clone(),
                            module_id: module.id.clone(),
                            enabled: false,
                        },
                    ));
                }
            }
        } else if propellant_kg < threshold {
            for &module_idx in station.modules_with_role(support_role) {
                let module = &station.core.modules[module_idx];
                if !module.enabled && module.wear.wear < 1.0 {
                    commands.push(make_cmd(
                        ctx.owner,
                        ctx.state.meta.tick,
                        ctx.next_id,
                        Command::SetModuleEnabled {
                            station_id: ctx.station_id.clone(),
                            module_id: module.id.clone(),
                            enabled: true,
                        },
                    ));
                }
            }
        }

        commands
    }
}
