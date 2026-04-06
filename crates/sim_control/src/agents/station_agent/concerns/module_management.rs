use std::collections::HashSet;

use sim_core::{Command, CommandEnvelope, InventoryItem, ModuleKindState, SlotType};

use crate::behaviors::make_cmd;

use super::super::StationConcern;
use super::super::StationContext;

/// 1. Install modules from inventory, re-enable disabled modules (except
///    propellant-role and max-wear), set processor thresholds.
pub(crate) struct ModuleManagement;

/// Shed load during power deficit or re-enable modules during surplus.
fn manage_power(
    station: &sim_core::StationState,
    ctx: &mut StationContext,
    commands: &mut Vec<CommandEnvelope>,
) {
    let has_power_gen = station.core.power.generated_kw > 0.0;
    let deficit_kw = station.core.power.deficit_kw;
    if has_power_gen && deficit_kw > ctx.state.strategy_config.power_deficit_threshold_kw {
        // Power deficit: disable least-critical consumers to shed load.
        // Uses power_priority() — None = infrastructure (never shed), lower = shed first.
        let mut shedding_candidates: Vec<(usize, f32, u8)> = station
            .core
            .modules
            .iter()
            .enumerate()
            .filter_map(|(index, module)| {
                if !module.enabled {
                    return None;
                }
                let def = ctx.content.module_defs.get(&module.def_id)?;
                let priority = def.power_priority()?; // None = infrastructure, skip
                if def.power_consumption_per_run <= 0.0 {
                    return None;
                }
                // Never shed propellant pipeline modules
                if ctx
                    .content
                    .module_has_role(&module.def_id, &ctx.content.autopilot.propellant_role)
                {
                    return None;
                }
                Some((index, def.power_consumption_per_run, priority))
            })
            .collect();
        // Sort ascending: lowest priority number = least critical = shed first
        shedding_candidates.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));

        let mut remaining_deficit = deficit_kw;
        for (index, power_kw, _) in &shedding_candidates {
            if remaining_deficit <= 0.0 {
                break;
            }
            let module = &station.core.modules[*index];
            commands.push(make_cmd(
                ctx.owner,
                ctx.state.meta.tick,
                ctx.next_id,
                Command::SetModuleEnabled {
                    facility_id: ctx.station_id.clone().into(),
                    module_id: module.id.clone(),
                    enabled: false,
                },
            ));
            remaining_deficit -= power_kw;
        }
    } else {
        // No deficit (or no power infrastructure): re-enable disabled modules.
        // When power infra exists, respect headroom; otherwise re-enable all.
        let mut available_headroom =
            station.core.power.generated_kw - station.core.power.consumed_kw;
        for module in &station.core.modules {
            if !module.enabled
                && module.wear.wear < 1.0
                && !ctx
                    .content
                    .module_has_role(&module.def_id, &ctx.content.autopilot.propellant_role)
            {
                let power_cost = ctx
                    .content
                    .module_defs
                    .get(&module.def_id)
                    .map_or(0.0, |d| d.power_consumption_per_run);
                if !has_power_gen || power_cost <= available_headroom || power_cost <= 0.0 {
                    commands.push(make_cmd(
                        ctx.owner,
                        ctx.state.meta.tick,
                        ctx.next_id,
                        Command::SetModuleEnabled {
                            facility_id: ctx.station_id.clone().into(),
                            module_id: module.id.clone(),
                            enabled: true,
                        },
                    ));
                    available_headroom -= power_cost;
                }
            }
        }
    }
}

/// Frameless install path: queue an `InstallModule` command for every
/// `InventoryItem::Module` on the station. Preserves legacy behavior for
/// stations that do not have a frame assigned (test fixtures, legacy saves).
fn enqueue_frameless_installs(
    station: &sim_core::StationState,
    ctx: &mut StationContext,
    commands: &mut Vec<CommandEnvelope>,
) {
    for item in &station.core.inventory {
        if let InventoryItem::Module { item_id, .. } = item {
            commands.push(make_cmd(
                ctx.owner,
                ctx.state.meta.tick,
                ctx.next_id,
                Command::InstallModule {
                    facility_id: ctx.station_id.clone().into(),
                    module_item_id: item_id.clone(),
                    slot_index: None,
                },
            ));
        }
    }
}

/// Framed install path: for each inventory module, pick a specific slot on
/// the station frame that is (a) compatible with the module's
/// `compatible_slots` and (b) not already occupied by an existing module
/// or by a slot the autopilot just claimed in this batch. Modules with no
/// compatible free slot are silently skipped until a slot opens up.
///
/// Sending an explicit `slot_index: Some(idx)` keeps the autopilot and the
/// handler in sync — without this, two items in the same tick could both
/// auto-find the same slot and the second one would be rejected by the
/// handler with a `NoCompatibleSlot` event.
fn enqueue_framed_installs(
    station: &sim_core::StationState,
    ctx: &mut StationContext,
    commands: &mut Vec<CommandEnvelope>,
) {
    let Some(frame_id) = station.frame_id.as_ref() else {
        return;
    };
    let Some(frame) = ctx.content.frames.get(frame_id) else {
        // Frame references a missing catalog entry — fall back to the
        // frameless path so content drops do not wedge the autopilot.
        enqueue_frameless_installs(station, ctx, commands);
        return;
    };

    // Slots that are already physically occupied by an existing module.
    let mut claimed: HashSet<usize> = station
        .core
        .modules
        .iter()
        .filter_map(|m| m.slot_index)
        .collect();

    for item in &station.core.inventory {
        let InventoryItem::Module {
            item_id,
            module_def_id,
        } = item
        else {
            continue;
        };
        let Some(def) = ctx.content.module_defs.get(module_def_id) else {
            continue;
        };
        let Some(slot_idx) = find_free_slot(&frame.slots, &def.compatible_slots, &claimed) else {
            // No compatible free slot this tick — wait for one to open up.
            continue;
        };
        claimed.insert(slot_idx);
        commands.push(make_cmd(
            ctx.owner,
            ctx.state.meta.tick,
            ctx.next_id,
            Command::InstallModule {
                facility_id: ctx.station_id.clone().into(),
                module_item_id: item_id.clone(),
                slot_index: Some(slot_idx),
            },
        ));
    }
}

/// Return the first slot index that is compatible with `compatible_slots`
/// and not already in `claimed`. Matches the handler's first-fit policy so
/// the autopilot and handler agree on slot assignments.
fn find_free_slot(
    frame_slots: &[sim_core::SlotDef],
    compatible_slots: &[SlotType],
    claimed: &HashSet<usize>,
) -> Option<usize> {
    for (idx, slot) in frame_slots.iter().enumerate() {
        if claimed.contains(&idx) {
            continue;
        }
        if compatible_slots.contains(&slot.slot_type) {
            return Some(idx);
        }
    }
    None
}

impl StationConcern for ModuleManagement {
    fn name(&self) -> &'static str {
        "module_management"
    }
    fn should_run(&self, _ctx: &StationContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        let mut commands = Vec::new();

        // SF-06: Generate slot-aware install commands.
        //
        // For framed stations, we track which slots are already occupied
        // by existing modules *plus* any slots the autopilot has claimed
        // earlier in this same command batch. This prevents two items in
        // a single tick from both racing for the same slot — the handler
        // would reject the second one with a NoCompatibleSlot event, but
        // skipping the command up front keeps the emit pipeline quiet and
        // the autopilot easier to reason about.
        //
        // For frameless stations, fall back to the legacy unlimited-slot
        // behavior: queue every inventory module without any checks.
        if station.frame_id.is_some() {
            enqueue_framed_installs(station, ctx, &mut commands);
        } else {
            enqueue_frameless_installs(station, ctx, &mut commands);
        }

        manage_power(station, ctx, &mut commands);

        for module in &station.core.modules {
            if let ModuleKindState::Processor(processor_state) = &module.kind_state {
                if processor_state.threshold_kg == 0.0 {
                    commands.push(make_cmd(
                        ctx.owner,
                        ctx.state.meta.tick,
                        ctx.next_id,
                        Command::SetModuleThreshold {
                            facility_id: ctx.station_id.clone().into(),
                            module_id: module.id.clone(),
                            threshold_kg: ctx.state.strategy_config.refinery_threshold_kg,
                        },
                    ));
                }
            }
        }

        commands
    }
}
