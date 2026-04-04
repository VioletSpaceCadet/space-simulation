use sim_core::{trade, Command, CommandEnvelope, TradeItemSpec};

use crate::agents::DecisionRecord;
use crate::behaviors::make_cmd;

use super::super::{has_unsatisfied_crew_need, StationConcern, StationContext};

/// 4. Recruit crew when demand exceeds supply.
pub(in crate::agents) struct CrewRecruitment;

impl StationConcern for CrewRecruitment {
    fn name(&self) -> &'static str {
        "crew_recruitment"
    }
    fn should_run(&self, ctx: &StationContext) -> bool {
        ctx.trade_import_unlocked
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        if !has_unsatisfied_crew_need(station, ctx.content) {
            return Vec::new();
        }

        let tick = ctx.state.meta.tick;
        let mut commands = Vec::new();

        let mut demand: std::collections::BTreeMap<sim_core::CrewRole, u32> =
            std::collections::BTreeMap::new();
        for module in &station.core.modules {
            if !module.enabled {
                continue;
            }
            let Some(def) = ctx.content.module_defs.get(&module.def_id) else {
                continue;
            };
            for (role, &count) in &def.crew_requirement {
                *demand.entry(role.clone()).or_insert(0) += count;
            }
        }

        for (role, needed) in &demand {
            let supply = station.core.crew.get(role).copied().unwrap_or(0);
            if supply >= *needed {
                continue;
            }
            let shortfall = needed - supply;
            let item_spec = TradeItemSpec::Crew {
                role: role.clone(),
                count: shortfall,
            };
            let Some(cost) =
                trade::compute_import_cost(&item_spec, &ctx.content.pricing, ctx.content)
            else {
                continue;
            };
            let budget_cap = ctx.state.balance * ctx.content.autopilot.budget_cap_fraction;
            if cost > budget_cap {
                continue;
            }
            // Salary projection: skip if hiring would cause bankruptcy within projection window
            let hours_per_tick = f64::from(ctx.content.constants.minutes_per_tick) / 60.0;
            let projection_ticks = ctx
                .content
                .constants
                .game_minutes_to_ticks(ctx.content.autopilot.crew_hire_projection_minutes);
            let current_salary_per_tick: f64 = ctx
                .state
                .stations
                .values()
                .flat_map(|s| s.core.crew.iter())
                .map(|(r, &c)| {
                    ctx.content
                        .crew_roles
                        .get(r)
                        .map_or(0.0, |d| d.salary_per_hour * f64::from(c) * hours_per_tick)
                })
                .sum();
            let new_hire_salary_per_tick = ctx.content.crew_roles.get(role).map_or(0.0, |d| {
                d.salary_per_hour * f64::from(shortfall) * hours_per_tick
            });
            let projected = ctx.state.balance
                - cost
                - (current_salary_per_tick + new_hire_salary_per_tick) * projection_ticks as f64;
            if projected < 0.0 {
                continue;
            }
            if let Some(ref mut log) = ctx.decisions {
                log.push(DecisionRecord {
                    tick,
                    agent: format!("station:{}", ctx.station_id.0),
                    concern: "crew_recruitment".to_string(),
                    decision_type: "recruit".to_string(),
                    chosen_id: role.0.clone(),
                    chosen_score: f64::from(shortfall),
                    alt_1_id: String::new(),
                    alt_1_score: 0.0,
                    alt_2_id: String::new(),
                    alt_2_score: 0.0,
                    alt_3_id: String::new(),
                    alt_3_score: 0.0,
                    context_json: format!(
                        "{{\"cost\":{cost},\"budget_cap\":{budget_cap},\"projected_balance\":{projected}}}",
                    ),
                });
            }
            commands.push(make_cmd(
                ctx.owner,
                tick,
                ctx.next_id,
                Command::Import {
                    facility_id: ctx.station_id.clone().into(),
                    item_spec,
                },
            ));
        }

        commands
    }
}
