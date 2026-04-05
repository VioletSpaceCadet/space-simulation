use sim_core::{
    trade, Command, CommandEnvelope, ComponentId, InventoryItem, LaunchPayload, TechId,
    TradeItemSpec,
};

use crate::behaviors::make_cmd;

use super::super::{GroundFacilityConcern, GroundFacilityContext};

/// Manages satellite lifecycle: imports components, launches satellites to orbit,
/// and queues replacements for aging satellites.
///
/// Execution flow per tick:
/// 1. Check if satellite tech is unlocked — early exit if not.
/// 2. Determine which satellite types are needed (not yet deployed, or nearing failure).
/// 3. If a needed satellite component is in inventory and a pad + rocket are available, launch it.
/// 4. If a needed satellite component is NOT in inventory, import it.
///
/// One action per tick (import or launch) to avoid budget spikes.
pub(in crate::agents) struct SatelliteManagement;

impl GroundFacilityConcern for SatelliteManagement {
    fn name(&self) -> &'static str {
        "satellite_management"
    }

    fn should_run(&self, ctx: &GroundFacilityContext) -> bool {
        // Skip if no satellite priority configured.
        if ctx.content.autopilot.satellite_priority.is_empty() {
            return false;
        }
        // Skip if satellite tech not unlocked.
        let tech = &ctx.content.autopilot.satellite_tech;
        if !tech.is_empty() && !ctx.state.research.unlocked.contains(&TechId(tech.clone())) {
            return false;
        }
        true
    }

    fn generate(&mut self, ctx: &mut GroundFacilityContext) -> Vec<CommandEnvelope> {
        let Some(facility) = ctx.state.ground_facilities.get(ctx.facility_id) else {
            return Vec::new();
        };

        // Determine which satellite type is needed next.
        let Some(needed_def_id) = find_needed_satellite(ctx) else {
            return Vec::new();
        };

        // Check if we already have the satellite component in inventory.
        let component_in_inventory = facility.core.inventory.iter().any(|item| {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = item
            {
                component_id.0 == needed_def_id && *count > 0
            } else {
                false
            }
        });

        if component_in_inventory {
            // Try to launch it.
            try_launch_satellite(ctx, &needed_def_id)
        } else {
            // Try to import the satellite component.
            try_import_satellite_component(ctx, &needed_def_id)
        }
    }
}

/// Find the next satellite type that needs deploying, in priority order.
/// Returns `None` if all priorities are satisfied.
fn find_needed_satellite(ctx: &GroundFacilityContext) -> Option<String> {
    let replacement_wear = ctx.content.autopilot.satellite_replacement_wear;

    for sat_def_id in &ctx.content.autopilot.satellite_priority {
        let Some(sat_def) = ctx.content.satellite_defs.get(sat_def_id.as_str()) else {
            continue;
        };

        // Check tech requirement for this specific satellite type.
        if let Some(ref required_tech) = sat_def.required_tech {
            if !ctx.state.research.unlocked.contains(required_tech) {
                continue;
            }
        }

        let sat_type = &sat_def.satellite_type;

        // Count active satellites of this type (enabled and not near failure).
        let active_count = ctx
            .state
            .satellites
            .values()
            .filter(|s| s.satellite_type == *sat_type && s.enabled && s.wear < replacement_wear)
            .count();

        // For now, ensure at least 1 healthy satellite of each priority type.
        if active_count == 0 {
            // Also check if one is already in transit (launch pending).
            let in_transit = ctx
                .state
                .ground_facilities
                .values()
                .flat_map(|f| &f.launch_transits)
                .any(|t| {
                    matches!(&t.payload, LaunchPayload::Satellite { satellite_def_id }
                        if satellite_def_id == sat_def_id)
                });
            if !in_transit {
                return Some(sat_def_id.clone());
            }
        }
    }

    None
}

/// Issue a Launch command with a Satellite payload.
fn try_launch_satellite(
    ctx: &mut GroundFacilityContext,
    satellite_def_id: &str,
) -> Vec<CommandEnvelope> {
    let rocket_id = &ctx.content.autopilot.satellite_launch_rocket;

    // Validate rocket exists and tech is unlocked.
    let Some(rocket_def) = ctx.content.rocket_defs.get(rocket_id.as_str()) else {
        return Vec::new();
    };
    if let Some(ref required_tech) = rocket_def.required_tech {
        if !ctx.state.research.unlocked.contains(required_tech) {
            return Vec::new();
        }
    }

    // Check satellite mass fits rocket.
    let Some(sat_def) = ctx.content.satellite_defs.get(satellite_def_id) else {
        return Vec::new();
    };
    if sat_def.mass_kg > rocket_def.payload_capacity_kg {
        return Vec::new();
    }

    // Check budget: base launch cost + fuel cost.
    let fuel_cost = f64::from(rocket_def.fuel_kg) * ctx.content.constants.launch_fuel_cost_per_kg;
    let total_cost = rocket_def.base_launch_cost + fuel_cost;
    if total_cost > ctx.state.balance * ctx.content.autopilot.budget_cap_fraction {
        return Vec::new();
    }

    // Check fuel availability.
    let Some(facility) = ctx.state.ground_facilities.get(ctx.facility_id) else {
        return Vec::new();
    };
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

    // Check pad availability.
    let pad_ok = facility.core.modules.iter().any(|module| {
        if !module.enabled {
            return false;
        }
        let Some(def) = ctx.content.module_defs.get(&module.def_id) else {
            return false;
        };
        let sim_core::ModuleBehaviorDef::LaunchPad(pad_def) = &def.behavior else {
            return false;
        };
        let sim_core::ModuleKindState::LaunchPad(pad_state) = &module.kind_state else {
            return false;
        };
        pad_state.available && pad_def.max_payload_kg >= rocket_def.payload_capacity_kg
    });
    if !pad_ok {
        return Vec::new();
    }

    // Pick a destination — use the facility's own position (orbit above ground).
    let destination = facility.position.clone();

    vec![make_cmd(
        ctx.owner,
        ctx.state.meta.tick,
        ctx.next_id,
        Command::Launch {
            facility_id: ctx.facility_id.clone(),
            rocket_def_id: rocket_id.clone(),
            payload: LaunchPayload::Satellite {
                satellite_def_id: satellite_def_id.to_string(),
            },
            destination,
        },
    )]
}

/// Import a satellite component via trade.
fn try_import_satellite_component(
    ctx: &mut GroundFacilityContext,
    satellite_def_id: &str,
) -> Vec<CommandEnvelope> {
    let item_spec = TradeItemSpec::Component {
        component_id: ComponentId(satellite_def_id.to_string()),
        count: 1,
    };

    let Some(cost) = trade::compute_import_cost(&item_spec, &ctx.content.pricing, ctx.content)
    else {
        return Vec::new();
    };
    if cost > ctx.state.balance * ctx.content.autopilot.budget_cap_fraction {
        return Vec::new();
    }

    vec![make_cmd(
        ctx.owner,
        ctx.state.meta.tick,
        ctx.next_id,
        Command::Import {
            facility_id: ctx.facility_id.clone().into(),
            item_spec,
        },
    )]
}
