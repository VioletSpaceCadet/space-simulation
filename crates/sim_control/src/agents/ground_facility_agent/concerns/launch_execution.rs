use sim_core::{
    Command, CommandEnvelope, InventoryItem, LaunchPayload, ModuleBehaviorDef, ModuleKindState,
};

use crate::behaviors::make_cmd;

use super::super::{GroundFacilityConcern, GroundFacilityContext};

/// Launch rockets when pad + rocket component + fuel are available.
///
/// Picks the first available rocket component in inventory, verifies
/// pad availability and fuel, then issues a Launch command with an
/// empty supplies payload (station kit launches deferred to later).
pub(in crate::agents) struct LaunchExecution;

impl GroundFacilityConcern for LaunchExecution {
    fn name(&self) -> &'static str {
        "launch_execution"
    }
    fn should_run(&self, _ctx: &GroundFacilityContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut GroundFacilityContext) -> Vec<CommandEnvelope> {
        let Some(facility) = ctx.state.ground_facilities.get(ctx.facility_id) else {
            return Vec::new();
        };

        // Find the first rocket component in inventory. Uses first-found order
        // (BTreeMap iteration = alphabetical by ID).
        let rocket_component = facility.core.inventory.iter().find_map(|item| {
            if let InventoryItem::Component { component_id, .. } = item {
                // Match rocket component IDs against rocket_defs.
                if ctx.content.rocket_defs.contains_key(&component_id.0) {
                    return Some(component_id.0.clone());
                }
            }
            None
        });
        let Some(rocket_def_id) = rocket_component else {
            return Vec::new();
        };

        let Some(rocket_def) = ctx.content.rocket_defs.get(&rocket_def_id) else {
            return Vec::new();
        };

        // Check tech gate.
        if let Some(ref tech_id) = rocket_def.required_tech {
            if !ctx.state.research.unlocked.contains(tech_id) {
                return Vec::new();
            }
        }

        // Check fuel availability.
        let fuel_element = &ctx.content.constants.launch_fuel_element;
        let available_fuel: f32 = facility
            .core
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == fuel_element => Some(*kg),
                _ => None,
            })
            .sum();
        if available_fuel < rocket_def.fuel_kg {
            return Vec::new();
        }

        // Check balance for launch cost.
        let fuel_cost =
            f64::from(rocket_def.fuel_kg) * ctx.content.constants.launch_fuel_cost_per_kg;
        let total_cost = rocket_def.base_launch_cost + fuel_cost;
        if total_cost > ctx.state.balance * ctx.state.strategy_config.budget_cap_fraction {
            return Vec::new();
        }

        // Pick payload. StationKit (5000 kg) only if no stations exist AND rocket
        // has enough capacity. Otherwise send empty supplies.
        let station_kit_mass = 5000.0_f32;
        let payload = if ctx.state.stations.is_empty()
            && rocket_def.payload_capacity_kg >= station_kit_mass
        {
            LaunchPayload::StationKit
        } else {
            LaunchPayload::Supplies(vec![])
        };

        // Use first station position as destination, or facility position for StationKit.
        let destination = ctx
            .state
            .stations
            .values()
            .next()
            .map_or_else(|| facility.position.clone(), |s| s.position.clone());

        // Verify pad can handle this rocket's capacity.
        let pad_ok = facility.core.modules.iter().any(|module| {
            if !module.enabled {
                return false;
            }
            let Some(def) = ctx.content.module_defs.get(&module.def_id) else {
                return false;
            };
            let ModuleBehaviorDef::LaunchPad(pad_def) = &def.behavior else {
                return false;
            };
            let ModuleKindState::LaunchPad(pad_state) = &module.kind_state else {
                return false;
            };
            pad_state.available && pad_def.max_payload_kg >= rocket_def.payload_capacity_kg
        });
        if !pad_ok {
            return Vec::new();
        }

        vec![make_cmd(
            ctx.owner,
            ctx.state.meta.tick,
            ctx.next_id,
            Command::Launch {
                facility_id: ctx.facility_id.clone(),
                rocket_def_id,
                payload,
                destination,
            },
        )]
    }
}
