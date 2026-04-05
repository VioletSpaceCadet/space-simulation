use sim_core::{Command, CommandEnvelope, ModuleBehaviorDef};

use crate::behaviors::make_cmd;

use super::super::{GroundFacilityConcern, GroundFacilityContext};

/// Enable/disable sensors based on operating cost budget.
///
/// If total per-tick operating cost across the facility exceeds the
/// configured fraction of balance, disables the most expensive sensors
/// first. Re-enables disabled sensors when budget allows.
pub(in crate::agents) struct SensorBudget;

impl GroundFacilityConcern for SensorBudget {
    fn name(&self) -> &'static str {
        "sensor_budget"
    }
    fn should_run(&self, _ctx: &GroundFacilityContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut GroundFacilityContext) -> Vec<CommandEnvelope> {
        let Some(facility) = ctx.state.ground_facilities.get(ctx.facility_id) else {
            return Vec::new();
        };

        let max_opex_per_tick = ctx.state.balance * ctx.content.autopilot.ground_opex_max_fraction;

        // Collect all sensor modules with their operating costs.
        let mut sensors: Vec<(usize, f64, bool)> = facility
            .core
            .modules
            .iter()
            .enumerate()
            .filter_map(|(index, module)| {
                let def = ctx.content.module_defs.get(&module.def_id)?;
                if !matches!(&def.behavior, ModuleBehaviorDef::SensorArray(_)) {
                    return None;
                }
                Some((index, def.operating_cost_per_tick, module.enabled))
            })
            .collect();

        let current_opex: f64 = sensors
            .iter()
            .filter(|(_, _, enabled)| *enabled)
            .map(|(_, cost, _)| cost)
            .sum();

        let mut commands = Vec::new();

        if current_opex > max_opex_per_tick {
            // Over budget: disable most expensive sensors first.
            // Sort descending by cost (disable expensive first).
            sensors.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let mut running_opex = current_opex;
            for &(index, cost, enabled) in &sensors {
                if running_opex <= max_opex_per_tick {
                    break;
                }
                if !enabled {
                    continue;
                }
                let module = &facility.core.modules[index];
                commands.push(make_cmd(
                    ctx.owner,
                    ctx.state.meta.tick,
                    ctx.next_id,
                    Command::SetModuleEnabled {
                        facility_id: ctx.facility_id.clone().into(),
                        module_id: module.id.clone(),
                        enabled: false,
                    },
                ));
                running_opex -= cost;
            }
        } else {
            // Under budget: re-enable disabled sensors if headroom allows.
            // Sort ascending by cost (enable cheapest first).
            sensors.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let mut running_opex = current_opex;
            for &(index, cost, enabled) in &sensors {
                if enabled {
                    continue;
                }
                // Skip max-wear modules
                if facility.core.modules[index].wear.wear >= 1.0 {
                    continue;
                }
                if running_opex + cost <= max_opex_per_tick {
                    let module = &facility.core.modules[index];
                    commands.push(make_cmd(
                        ctx.owner,
                        ctx.state.meta.tick,
                        ctx.next_id,
                        Command::SetModuleEnabled {
                            facility_id: ctx.facility_id.clone().into(),
                            module_id: module.id.clone(),
                            enabled: true,
                        },
                    ));
                    running_opex += cost;
                }
            }
        }

        commands
    }
}
