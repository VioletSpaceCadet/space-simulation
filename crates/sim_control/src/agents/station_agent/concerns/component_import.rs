use sim_core::{
    trade, Command, CommandEnvelope, ComponentId, InputAmount, InputFilter, InventoryItem,
    ModuleBehaviorDef, TechId, TradeItemSpec,
};

use crate::behaviors::make_cmd;

use super::super::{StationConcern, StationContext};

/// 5. Import thruster components for shipyard.
pub(in crate::agents) struct ComponentImport;

impl StationConcern for ComponentImport {
    fn name(&self) -> &'static str {
        "component_import"
    }
    fn should_run(&self, ctx: &StationContext) -> bool {
        ctx.trade_import_unlocked
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        let tech_unlocked = ctx.state.research.unlocked.contains(&TechId(
            ctx.content.autopilot.ship_construction_tech.clone(),
        ));
        if !tech_unlocked {
            return Vec::new();
        }

        // Skip if fleet is already at or above target size
        let fleet_count = ctx
            .state
            .ships
            .values()
            .filter(|s| s.owner == *ctx.owner)
            .count();
        if fleet_count >= ctx.state.strategy_config.fleet_size_target as usize {
            return Vec::new();
        }

        let shipyard_role = &ctx.content.autopilot.shipyard_role;
        let import_component = &ctx.content.autopilot.shipyard_import_component;

        // Find the shipyard recipe's component requirement
        let mut shipyard_defs: Vec<_> = ctx
            .content
            .module_defs
            .values()
            .filter(|def| def.roles.iter().any(|r| r == shipyard_role))
            .collect();
        shipyard_defs.sort_by(|a, b| a.id.cmp(&b.id));
        let required_components = shipyard_defs
            .first()
            .and_then(|def| match &def.behavior {
                ModuleBehaviorDef::Assembler(assembler_def) => assembler_def
                    .recipes
                    .first()
                    .and_then(|recipe_id| ctx.content.recipes.get(recipe_id)),
                _ => None,
            })
            .map_or(
                ctx.state.strategy_config.shipyard_component_count,
                |recipe| {
                    recipe
                        .inputs
                        .iter()
                        .find_map(|input| match (&input.filter, &input.amount) {
                            (InputFilter::Component(cid), InputAmount::Count(n))
                                if cid.0 == *import_component =>
                            {
                                Some(*n)
                            }
                            _ => None,
                        })
                        .unwrap_or(ctx.state.strategy_config.shipyard_component_count)
                },
            );

        let has_shipyard = station
            .modules_with_role(shipyard_role)
            .iter()
            .any(|&idx| station.core.modules[idx].enabled);
        if !has_shipyard {
            return Vec::new();
        }

        let component_count: u32 = station
            .core
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == *import_component => Some(*count),
                _ => None,
            })
            .sum();
        if component_count >= required_components {
            return Vec::new();
        }

        let needed = required_components - component_count;
        let item_spec = TradeItemSpec::Component {
            component_id: ComponentId(import_component.clone()),
            count: needed,
        };

        let Some(cost) = trade::compute_import_cost(&item_spec, &ctx.content.pricing, ctx.content)
        else {
            return Vec::new();
        };
        if cost > ctx.state.balance * ctx.state.strategy_config.budget_cap_fraction {
            return Vec::new();
        }

        vec![make_cmd(
            ctx.owner,
            ctx.state.meta.tick,
            ctx.next_id,
            Command::Import {
                facility_id: ctx.station_id.clone().into(),
                item_spec,
            },
        )]
    }
}
