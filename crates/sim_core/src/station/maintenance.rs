use crate::{
    Event, EventEnvelope, GameContent, GameState, InventoryItem, ModuleBehaviorDef,
    ModuleKindState, StationId,
};

#[allow(clippy::too_many_lines)]
pub(super) fn tick_maintenance_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let module_count = state
        .stations
        .get(station_id)
        .map_or(0, |s| s.modules.len());

    for module_idx in 0..module_count {
        let (interval, power_needed, repair_reduction, kit_cost, repair_threshold) = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            let module = &station.modules[module_idx];
            if !module.enabled || module.power_stalled {
                continue;
            }
            let Some(def) = content.module_defs.get(&module.def_id) else {
                continue;
            };
            let ModuleBehaviorDef::Maintenance(maint_def) = &def.behavior else {
                continue;
            };
            (
                maint_def.repair_interval_ticks,
                def.power_consumption_per_run,
                maint_def.wear_reduction_per_run,
                maint_def.repair_kit_cost,
                maint_def.repair_threshold,
            )
        };

        // Tick timer
        {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                ms.ticks_since_last_run += 1;
                if ms.ticks_since_last_run < interval {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Check power
        {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        // Find most worn module (not self, wear >= threshold), sorted by wear desc then ID asc for determinism
        let target = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            let self_id = &station.modules[module_idx].id;
            let mut candidates: Vec<(usize, f32, String)> = station
                .modules
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    m.id != *self_id && m.wear.wear >= repair_threshold && m.wear.wear > 0.0
                })
                .map(|(idx, m)| (idx, m.wear.wear, m.id.0.clone()))
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
            candidates.first().map(|(idx, _, _)| *idx)
        };

        let Some(target_idx) = target else {
            // Nothing worn — reset timer but don't consume kit
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Maintenance(ms) =
                    &mut station.modules[module_idx].kind_state
                {
                    ms.ticks_since_last_run = 0;
                }
            }
            continue;
        };

        // Consume repair kit
        let has_kit = {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            let kit_slot = station.inventory.iter_mut().find(|i| {
                matches!(i, InventoryItem::Component { component_id, count, .. }
                    if component_id.0 == "repair_kit" && *count >= kit_cost)
            });
            if let Some(InventoryItem::Component { count, .. }) = kit_slot {
                *count -= kit_cost;
                true
            } else {
                false
            }
        };

        if !has_kit {
            // Reset timer even if no kit
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Maintenance(ms) =
                    &mut station.modules[module_idx].kind_state
                {
                    ms.ticks_since_last_run = 0;
                }
            }
            continue;
        }

        // Remove empty component stacks
        if let Some(station) = state.stations.get_mut(station_id) {
            station
                .inventory
                .retain(|i| !matches!(i, InventoryItem::Component { count, .. } if *count == 0));
            station.invalidate_volume_cache();
        }

        // Apply repair
        let (target_module_id, wear_before, wear_after, kits_remaining) = {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            let target_module = &mut station.modules[target_idx];
            let wear_before = target_module.wear.wear;
            target_module.wear.wear = (target_module.wear.wear - repair_reduction).max(0.0);
            let wear_after = target_module.wear.wear;
            let target_module_id = target_module.id.clone();

            // Re-enable module if it was auto-disabled due to wear
            if !target_module.enabled && wear_after < 1.0 {
                target_module.enabled = true;
            }

            let kits_remaining: u32 = station
                .inventory
                .iter()
                .filter_map(|i| {
                    if let InventoryItem::Component {
                        component_id,
                        count,
                        ..
                    } = i
                    {
                        if component_id.0 == "repair_kit" {
                            Some(*count)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .sum();

            // Reset timer
            if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                ms.ticks_since_last_run = 0;
            }

            (target_module_id, wear_before, wear_after, kits_remaining)
        };

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::MaintenanceRan {
                station_id: station_id.clone(),
                target_module_id,
                wear_before,
                wear_after,
                repair_kits_remaining: kits_remaining,
            },
        ));
    }
}
