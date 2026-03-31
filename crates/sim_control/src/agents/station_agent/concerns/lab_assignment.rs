use std::collections::HashMap;

use sim_core::{
    Command, CommandEnvelope, ModuleBehaviorDef, ModuleKindState, ResearchDomain, TechId,
};

use crate::agents::DecisionRecord;
use crate::behaviors::{compute_sufficiency, make_cmd};

use super::super::StationConcern;
use super::super::StationContext;

/// Per-station cache for lab assignment decisions.
///
/// Maps research domains to eligible tech IDs. Rebuilt when the set of
/// unlocked techs changes.
#[derive(Default)]
pub(crate) struct LabAssignmentCache {
    /// domain → eligible tech IDs (prereqs met, not yet unlocked, needs this domain).
    pub(crate) cached_eligible: HashMap<ResearchDomain, Vec<TechId>>,
    /// Number of unlocked techs when cache was last built.
    pub(crate) last_unlocked_count: usize,
    /// Whether the cache has been initialized at all.
    pub(crate) initialized: bool,
}

/// 2. Assign unassigned labs to the highest-priority eligible tech.
#[derive(Default)]
pub(crate) struct LabAssignment {
    cache: LabAssignmentCache,
}

impl StationConcern for LabAssignment {
    fn name(&self) -> &'static str {
        "lab_assignment"
    }
    fn should_run(&self, _ctx: &StationContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        // Rebuild eligible tech cache when unlocked set changes.
        let unlocked_count = ctx.state.research.unlocked.len();
        if !self.cache.initialized || unlocked_count != self.cache.last_unlocked_count {
            self.cache.cached_eligible.clear();
            for tech in &ctx.content.techs {
                if ctx.state.research.unlocked.contains(&tech.id) {
                    continue;
                }
                if !tech
                    .prereqs
                    .iter()
                    .all(|p| ctx.state.research.unlocked.contains(p))
                {
                    continue;
                }
                for domain in tech.domain_requirements.keys() {
                    self.cache
                        .cached_eligible
                        .entry(domain.clone())
                        .or_default()
                        .push(tech.id.clone());
                }
            }
            self.cache.last_unlocked_count = unlocked_count;
            self.cache.initialized = true;
        }

        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        let mut commands = Vec::new();

        for module in &station.modules {
            let ModuleKindState::Lab(lab_state) = &module.kind_state else {
                continue;
            };
            if let Some(ref tech_id) = lab_state.assigned_tech {
                if !ctx.state.research.unlocked.contains(tech_id) {
                    continue;
                }
            }

            let Some(def) = ctx.content.module_defs.get(&module.def_id) else {
                continue;
            };
            let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else {
                continue;
            };

            let eligible = self
                .cache
                .cached_eligible
                .get(&lab_def.domain)
                .map_or(&[][..], |v| v.as_slice());
            let mut candidates: Vec<(TechId, f32)> = eligible
                .iter()
                .filter(|tid| !ctx.state.research.unlocked.contains(tid))
                .filter_map(|tid| {
                    let tech = ctx.content.techs.iter().find(|t| t.id == *tid)?;
                    let sufficiency =
                        compute_sufficiency(tech, ctx.state.research.evidence.get(tid));
                    Some((tid.clone(), sufficiency))
                })
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));

            if let Some((tech_id, score)) = candidates.first() {
                commands.push(make_cmd(
                    ctx.owner,
                    ctx.state.meta.tick,
                    ctx.next_id,
                    Command::AssignLabTech {
                        station_id: ctx.station_id.clone(),
                        module_id: module.id.clone(),
                        tech_id: Some(tech_id.clone()),
                    },
                ));
                if let Some(ref mut log) = ctx.decisions {
                    log.push(DecisionRecord {
                        tick: ctx.state.meta.tick,
                        agent: format!("station:{}", ctx.station_id.0),
                        concern: "lab_assignment".to_string(),
                        decision_type: "pick_tech".to_string(),
                        chosen_id: tech_id.0.clone(),
                        chosen_score: f64::from(*score),
                        alt_1_id: candidates
                            .get(1)
                            .map_or_else(String::new, |(t, _)| t.0.clone()),
                        alt_1_score: candidates.get(1).map_or(0.0, |(_, s)| f64::from(*s)),
                        alt_2_id: candidates
                            .get(2)
                            .map_or_else(String::new, |(t, _)| t.0.clone()),
                        alt_2_score: candidates.get(2).map_or(0.0, |(_, s)| f64::from(*s)),
                        alt_3_id: candidates
                            .get(3)
                            .map_or_else(String::new, |(t, _)| t.0.clone()),
                        alt_3_score: candidates.get(3).map_or(0.0, |(_, s)| f64::from(*s)),
                        context_json: format!(
                            "{{\"domain\":\"{:?}\",\"module\":\"{}\"}}",
                            lab_def.domain, module.id.0,
                        ),
                    });
                }
            }
        }

        commands
    }
}
