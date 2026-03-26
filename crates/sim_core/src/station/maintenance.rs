use crate::{
    Event, EventEnvelope, GameContent, GameState, InventoryItem, ModuleBehaviorDef, StationId,
};

pub(super) fn tick_maintenance_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    super::ensure_station_index(state, station_id, content);
    let indices: Vec<usize> = state
        .stations
        .get(station_id)
        .map(|s| s.module_type_index.maintenance.clone())
        .unwrap_or_default();

    for module_idx in indices {
        let Some(ctx) = super::extract_context(state, station_id, module_idx, content) else {
            continue;
        };

        let ModuleBehaviorDef::Maintenance(_) = &ctx.def.behavior else {
            continue;
        };

        if !super::should_run(state, &ctx) {
            continue;
        }

        let outcome = execute(&ctx, state, events);
        super::apply_run_result(state, &ctx, outcome, content, events);
    }
}

fn execute(
    ctx: &super::ModuleTickContext,
    state: &mut GameState,
    events: &mut Vec<EventEnvelope>,
) -> super::RunOutcome {
    // Extract maintenance-specific def fields
    let crate::ModuleBehaviorDef::Maintenance(maint_def) = &ctx.def.behavior else {
        return super::RunOutcome::Skipped { reset_timer: true };
    };
    let repair_reduction = maint_def.wear_reduction_per_run;
    let kit_cost = maint_def.repair_kit_cost;
    let repair_threshold = maint_def.repair_threshold;
    let component_id_str = &maint_def.maintenance_component_id;

    let current_tick = state.meta.tick;

    // Find most worn module (not self, wear >= threshold)
    let target_idx = {
        let Some(station) = state.stations.get(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: true };
        };
        let self_id = &station.modules[ctx.module_idx].id;
        let mut candidates: Vec<(usize, f32, String)> = station
            .modules
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.id != *self_id && m.wear.wear >= repair_threshold && m.wear.wear > 0.0
            })
            .map(|(module_index, m)| (module_index, m.wear.wear, m.id.0.clone()))
            .collect();
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
        match candidates.first() {
            Some((module_index, _, _)) => *module_index,
            None => return super::RunOutcome::Skipped { reset_timer: true },
        }
    };

    // Consume repair kit
    let has_kit = {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: true };
        };
        let kit_slot = station.inventory.iter_mut().find(|i| {
            matches!(i, InventoryItem::Component { component_id, count, .. }
                if component_id.0 == *component_id_str && *count >= kit_cost)
        });
        if let Some(InventoryItem::Component { count, .. }) = kit_slot {
            *count -= kit_cost;
            true
        } else {
            false
        }
    };

    if !has_kit {
        return super::RunOutcome::Skipped { reset_timer: true };
    }

    // Remove empty component stacks
    if let Some(station) = state.stations.get_mut(&ctx.station_id) {
        station
            .inventory
            .retain(|i| !matches!(i, InventoryItem::Component { count, .. } if *count == 0));
    }

    // Apply repair
    let (target_module_id, wear_before, wear_after, kits_remaining) = {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: true };
        };
        let target_module = &mut station.modules[target_idx];
        let wear_before = target_module.wear.wear;
        target_module.wear.wear = (target_module.wear.wear - repair_reduction).max(0.0);
        let wear_after = target_module.wear.wear;
        let target_module_id = target_module.id.clone();

        // Re-enable module if it was auto-disabled due to wear
        if !target_module.enabled && wear_after < 1.0 {
            target_module.enabled = true;
            station.invalidate_power_cache();
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
                    if component_id.0 == *component_id_str {
                        Some(*count)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .sum();

        (target_module_id, wear_before, wear_after, kits_remaining)
    };

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::MaintenanceRan {
            station_id: ctx.station_id.clone(),
            target_module_id,
            wear_before,
            wear_after,
            repair_kits_remaining: kits_remaining,
        },
    ));

    super::RunOutcome::Completed
}
