use sim_core::{trade, Command, CommandEnvelope, ModuleBehaviorDef, TradeItemSpec};

use crate::behaviors::make_cmd;

use super::super::{GroundFacilityConcern, GroundFacilityContext};

/// Buy sensor modules in priority order (optical first, then radio).
///
/// Purchases one sensor per tick at most. Skips if the facility already has
/// the sensor installed or in inventory.
pub(in crate::agents) struct SensorPurchase;

impl GroundFacilityConcern for SensorPurchase {
    fn name(&self) -> &'static str {
        "sensor_purchase"
    }
    fn should_run(&self, ctx: &GroundFacilityContext) -> bool {
        !ctx.content.autopilot.ground_sensor_modules.is_empty()
    }
    fn generate(&mut self, ctx: &mut GroundFacilityContext) -> Vec<CommandEnvelope> {
        let Some(facility) = ctx.state.ground_facilities.get(ctx.facility_id) else {
            return Vec::new();
        };

        for sensor_def_id in &ctx.content.autopilot.ground_sensor_modules {
            // Skip if already installed
            let already_installed = facility
                .core
                .modules
                .iter()
                .any(|m| m.def_id == *sensor_def_id);
            if already_installed {
                continue;
            }

            // Skip if already in inventory (pending install)
            let in_inventory = facility.core.inventory.iter().any(|item| {
                matches!(item, sim_core::InventoryItem::Module { module_def_id, .. } if module_def_id == sensor_def_id)
            });
            if in_inventory {
                continue;
            }

            // Verify this is actually a sensor module
            let Some(def) = ctx.content.module_defs.get(sensor_def_id) else {
                continue;
            };
            if !matches!(&def.behavior, ModuleBehaviorDef::SensorArray(_)) {
                continue;
            }

            // Skip if the module's required_tech is not unlocked — otherwise
            // we'd import it and fail to install every tick.
            if let Some(ref required_tech) = def.required_tech {
                if !ctx.state.research.unlocked.contains(required_tech) {
                    continue;
                }
            }

            // Check budget
            let item_spec = TradeItemSpec::Module {
                module_def_id: sensor_def_id.clone(),
            };
            let Some(cost) =
                trade::compute_import_cost(&item_spec, &ctx.content.pricing, ctx.content)
            else {
                continue;
            };
            if cost > ctx.state.balance * ctx.content.autopilot.budget_cap_fraction {
                continue;
            }

            // Buy one sensor per tick
            return vec![make_cmd(
                ctx.owner,
                ctx.state.meta.tick,
                ctx.next_id,
                Command::Import {
                    facility_id: ctx.facility_id.clone().into(),
                    item_spec,
                },
            )];
        }

        Vec::new()
    }
}
