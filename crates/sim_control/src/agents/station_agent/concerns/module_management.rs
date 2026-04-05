use sim_core::{Command, CommandEnvelope, InventoryItem, ModuleKindState};

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
    if has_power_gen && deficit_kw > ctx.content.autopilot.power_deficit_threshold_kw {
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

        for item in &station.core.inventory {
            if let InventoryItem::Module { item_id, .. } = item {
                commands.push(make_cmd(
                    ctx.owner,
                    ctx.state.meta.tick,
                    ctx.next_id,
                    Command::InstallModule {
                        facility_id: ctx.station_id.clone().into(),
                        module_item_id: item_id.clone(),
                        // SF-06 will add slot-aware selection; today the
                        // autopilot lets the handler auto-find a slot.
                        slot_index: None,
                    },
                ));
            }
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
                            threshold_kg: ctx.content.autopilot.refinery_threshold_kg,
                        },
                    ));
                }
            }
        }

        commands
    }
}
