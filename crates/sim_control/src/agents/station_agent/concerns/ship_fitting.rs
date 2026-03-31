use std::collections::HashMap;

use sim_core::{Command, CommandEnvelope, InventoryItem};

use crate::behaviors::{collect_idle_ships, make_cmd};

use super::super::{StationConcern, StationContext};

/// 9. Fit idle ships at this station with available modules.
pub(in crate::agents) struct ShipFitting;

impl StationConcern for ShipFitting {
    fn name(&self) -> &'static str {
        "ship_fitting"
    }
    fn should_run(&self, _ctx: &StationContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        let idle_ships = collect_idle_ships(ctx.state, ctx.owner);
        let mut consumed: HashMap<String, usize> = HashMap::new();
        let mut commands = Vec::new();

        for ship_id in &idle_ships {
            let Some(ship) = ctx.state.ships.get(ship_id) else {
                continue;
            };

            // Ship must be at this station
            if ship.position != station.position {
                continue;
            }

            let Some(template) = ctx.content.fitting_templates.get(&ship.hull_id) else {
                continue;
            };

            for entry in template {
                if ship
                    .fitted_modules
                    .iter()
                    .any(|fm| fm.slot_index == entry.slot_index)
                {
                    continue;
                }

                let module_def_id_str = &entry.module_def_id.0;
                let in_inventory = station
                    .inventory
                    .iter()
                    .filter(|item| {
                        matches!(item, InventoryItem::Module { module_def_id, .. } if module_def_id == module_def_id_str)
                    })
                    .count();
                let already_consumed = consumed.get(module_def_id_str).copied().unwrap_or(0);
                let available = in_inventory > already_consumed;

                if available {
                    *consumed.entry(module_def_id_str.clone()).or_insert(0) += 1;
                    commands.push(make_cmd(
                        ctx.owner,
                        ctx.state.meta.tick,
                        ctx.next_id,
                        Command::FitShipModule {
                            ship_id: ship_id.clone(),
                            slot_index: entry.slot_index,
                            module_def_id: entry.module_def_id.clone(),
                            station_id: ctx.station_id.clone(),
                        },
                    ));
                }
            }
        }

        commands
    }
}
