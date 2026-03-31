use sim_core::{Command, CommandEnvelope};

use crate::behaviors::make_cmd;

use super::super::{has_unsatisfied_crew_need, StationConcern, StationContext};

/// 3. Assign available crew to understaffed modules by priority.
pub(in crate::agents) struct CrewAssignment;

impl StationConcern for CrewAssignment {
    fn name(&self) -> &'static str {
        "crew_assignment"
    }
    fn should_run(&self, _ctx: &StationContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        if !has_unsatisfied_crew_need(station, ctx.content) || station.crew.is_empty() {
            return Vec::new();
        }

        let tick = ctx.state.meta.tick;
        let mut commands = Vec::new();

        let mut module_order: Vec<usize> = (0..station.modules.len()).collect();
        module_order.sort_by(|&a, &b| {
            station.modules[b]
                .module_priority
                .cmp(&station.modules[a].module_priority)
                .then_with(|| station.modules[a].id.0.cmp(&station.modules[b].id.0))
        });

        let mut available: std::collections::BTreeMap<sim_core::CrewRole, u32> =
            station.crew.clone();
        for module in &station.modules {
            for (role, &count) in &module.assigned_crew {
                if let Some(entry) = available.get_mut(role) {
                    *entry = entry.saturating_sub(count);
                }
            }
        }

        for &module_index in &module_order {
            let module = &station.modules[module_index];
            if !module.enabled || module.prev_crew_satisfied {
                continue;
            }
            let Some(def) = ctx.content.module_defs.get(&module.def_id) else {
                continue;
            };
            if def.crew_requirement.is_empty() {
                continue;
            }
            for (role, &needed) in &def.crew_requirement {
                let assigned = module.assigned_crew.get(role).copied().unwrap_or(0);
                if assigned >= needed {
                    continue;
                }
                let gap = needed - assigned;
                let can_assign = available.get(role).copied().unwrap_or(0).min(gap);
                if can_assign > 0 {
                    commands.push(make_cmd(
                        ctx.owner,
                        tick,
                        ctx.next_id,
                        Command::AssignCrew {
                            station_id: ctx.station_id.clone(),
                            module_id: module.id.clone(),
                            role: role.clone(),
                            count: can_assign,
                        },
                    ));
                    if let Some(entry) = available.get_mut(role) {
                        *entry -= can_assign;
                    }
                }
            }
        }

        commands
    }
}
