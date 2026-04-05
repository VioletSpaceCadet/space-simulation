use sim_core::{trade, Command, CommandEnvelope, ComponentId, InventoryItem, TradeItemSpec};

use crate::behaviors::make_cmd;

use super::super::{GroundFacilityConcern, GroundFacilityContext};

/// Buy rocket components (solid fuel grains, guidance units) when budget allows.
///
/// Purchases components needed for the cheapest buildable rocket recipe.
/// One purchase per tick to avoid draining balance.
pub(in crate::agents) struct ComponentPurchase;

impl GroundFacilityConcern for ComponentPurchase {
    fn name(&self) -> &'static str {
        "component_purchase"
    }
    fn should_run(&self, ctx: &GroundFacilityContext) -> bool {
        !ctx.content.autopilot.ground_sensor_modules.is_empty()
    }
    fn generate(&mut self, ctx: &mut GroundFacilityContext) -> Vec<CommandEnvelope> {
        let Some(facility) = ctx.state.ground_facilities.get(ctx.facility_id) else {
            return Vec::new();
        };

        // Buy components needed for sounding rocket (simplest recipe):
        // 2x solid_fuel_grain + 1x guidance_unit
        let components_to_buy = [("solid_fuel_grain", 2u32), ("guidance_unit", 1u32)];

        for (component_id, target_count) in &components_to_buy {
            let current_count: u32 = facility
                .core
                .inventory
                .iter()
                .filter_map(|item| match item {
                    InventoryItem::Component {
                        component_id: cid,
                        count,
                        ..
                    } if cid.0 == *component_id => Some(*count),
                    _ => None,
                })
                .sum();

            if current_count >= *target_count {
                continue;
            }

            let needed = *target_count - current_count;
            let item_spec = TradeItemSpec::Component {
                component_id: ComponentId(component_id.to_string()),
                count: needed,
            };

            let Some(cost) =
                trade::compute_import_cost(&item_spec, &ctx.content.pricing, ctx.content)
            else {
                continue;
            };
            if cost > ctx.state.balance * ctx.content.autopilot.budget_cap_fraction {
                continue;
            }

            // One purchase per tick.
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
