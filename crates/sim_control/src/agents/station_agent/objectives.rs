use std::collections::BTreeMap;

use sim_core::{
    compute_entity_absolute, AsteroidId, GameContent, GameState, PrincipalId, ShipId, SiteId,
    StationId, TechId,
};

use crate::agents::ship_agent::ShipAgent;
use crate::agents::DecisionRecord;
use crate::behaviors::{
    collect_deep_scan_candidates, collect_idle_ships, deposit_priority, element_mining_value,
    should_opportunistic_refuel, station_has_module_with_role, total_element_inventory,
};
use crate::objectives::ShipObjective;

use super::StationAgent;

/// Iterator state for ship objective candidate lists.
struct ObjectiveCandidates<'a> {
    mine: &'a std::slice::Iter<'a, AsteroidId>,
    site: &'a std::slice::Iter<'a, SiteId>,
    deep_scan: &'a std::slice::Iter<'a, AsteroidId>,
}

/// Log a ship objective assignment decision.
fn log_objective_decision(
    log: &mut Vec<DecisionRecord>,
    tick: u64,
    station_id: &StationId,
    obj: &ShipObjective,
    priority: &str,
    candidates: &ObjectiveCandidates,
) {
    let (chosen_id, decision_type) = match obj {
        ShipObjective::Mine { asteroid_id } => (asteroid_id.0.clone(), "assign_mine"),
        ShipObjective::Survey { site_id } => (site_id.0.clone(), "assign_survey"),
        ShipObjective::DeepScan { asteroid_id } => (asteroid_id.0.clone(), "assign_deep_scan"),
        ShipObjective::Deposit { station_id } => (station_id.0.clone(), "assign_deposit"),
        ShipObjective::Idle => (String::new(), "idle"),
    };
    // Peek at remaining alternatives from the matching iterator.
    // The _ arm reports deep_scan alternatives (only other scored type).
    let alts: Vec<String> = match priority {
        "Mine" => candidates
            .mine
            .clone()
            .take(3)
            .map(|id| id.0.clone())
            .collect(),
        "Survey" => candidates
            .site
            .clone()
            .take(3)
            .map(|id| id.0.clone())
            .collect(),
        _ => candidates
            .deep_scan
            .clone()
            .take(3)
            .map(|id| id.0.clone())
            .collect(),
    };
    log.push(DecisionRecord {
        tick,
        agent: format!("station:{}", station_id.0),
        concern: "ship_objectives".to_string(),
        decision_type: decision_type.to_string(),
        chosen_id,
        chosen_score: 0.0,
        alt_1_id: alts.first().cloned().unwrap_or_default(),
        alt_1_score: 0.0,
        alt_2_id: alts.get(1).cloned().unwrap_or_default(),
        alt_2_score: 0.0,
        alt_3_id: alts.get(2).cloned().unwrap_or_default(),
        alt_3_score: 0.0,
        context_json: format!(
            "{{\"mine_remaining\":{},\"survey_remaining\":{},\"deep_scan_remaining\":{}}}",
            candidates.mine.clone().count(),
            candidates.site.clone().count(),
            candidates.deep_scan.clone().count(),
        ),
    });
}

/// Survey sites sorted by distance from reference position (nearest first).
pub(in crate::agents) fn collect_survey_candidates(
    state: &GameState,
    reference_pos: &sim_core::Position,
) -> Vec<SiteId> {
    if state.scan_sites.is_empty() {
        return Vec::new();
    }
    let ref_abs = compute_entity_absolute(reference_pos, &state.body_cache);
    let mut decorated: Vec<(u128, SiteId)> = state
        .scan_sites
        .iter()
        .map(|site| {
            let dist = ref_abs
                .distance_squared(compute_entity_absolute(&site.position, &state.body_cache));
            (dist, site.id.clone())
        })
        .collect();
    decorated.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1 .0.cmp(&b.1 .0)));
    decorated.into_iter().map(|(_, id)| id).collect()
}

/// Mine candidates sorted by mining value (mass * element fraction), descending.
/// Volatile detection determines which element to prioritize.
pub(in crate::agents) fn collect_mine_candidates(
    state: &GameState,
    content: &GameContent,
) -> Vec<AsteroidId> {
    let propellant_role = &content.autopilot.propellant_role;
    let support_role = &content.autopilot.propellant_support_role;
    let has_propellant_module = station_has_module_with_role(state, propellant_role);
    let volatile_element = &content.autopilot.volatile_element;
    let propellant_element = &content.autopilot.propellant_element;
    let primary_element = &content.autopilot.primary_mining_element;
    let needs_volatiles = station_has_module_with_role(state, support_role)
        && (total_element_inventory(state, volatile_element)
            < content.constants.autopilot_volatile_threshold_kg
            || (has_propellant_module
                && total_element_inventory(state, propellant_element)
                    < content.constants.autopilot_lh2_threshold_kg));

    let sort_element = if needs_volatiles {
        volatile_element
    } else {
        primary_element
    };
    let mut decorated: Vec<(f32, AsteroidId)> = state
        .asteroids
        .values()
        .filter(|a| a.mass_kg > 0.0 && a.knowledge.composition.is_some())
        .map(|a| (element_mining_value(a, sort_element), a.id.clone()))
        .collect();
    decorated.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1 .0.cmp(&b.1 .0)));
    decorated.into_iter().map(|(_, id)| id).collect()
}

impl StationAgent {
    /// Assign objectives to idle ship agents owned by this station's owner.
    ///
    /// Ships can be at any position — the ship agent will generate Transit
    /// tasks to reach assigned targets. Uses shared-iterator deduplication (AD1)
    /// so no two ships target the same asteroid or scan site.
    ///
    /// Called separately from `generate()` because it mutates ship agents,
    /// not the command buffer.
    pub(crate) fn assign_ship_objectives(
        &self,
        ship_agents: &mut BTreeMap<ShipId, ShipAgent>,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        mut decisions: Option<&mut Vec<DecisionRecord>>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        // Collect idle ships owned by this station's owner with no current objective.
        // Ships can be at any position — the ship agent will generate Transit
        // tasks to reach assigned targets (fixes VIO-457: ships stranded after
        // completing tasks at remote locations).
        let idle_ships = collect_idle_ships(state, owner);
        let assignable: Vec<ShipId> = idle_ships
            .into_iter()
            .filter(|id| ship_agents.get(id).is_some_and(|a| a.objective.is_none()))
            .collect();

        if assignable.is_empty() {
            return;
        }

        let reference_pos = &station.position;

        let deep_scan_unlocked = state
            .research
            .unlocked
            .contains(&TechId(content.autopilot.deep_scan_tech.clone()));

        // Pre-compute sorted candidate lists (Schwartzian transforms)
        let deep_scan_candidates = collect_deep_scan_candidates(state, content, reference_pos);
        let survey_candidates = collect_survey_candidates(state, reference_pos);
        let mine_candidates = collect_mine_candidates(state, content);

        let mut next_deep_scan = deep_scan_candidates.iter();
        let mut next_site = survey_candidates.iter();
        let mut next_mine = mine_candidates.iter();

        // Assign objectives using shared iterators (AD1)
        for ship_id in assignable {
            let Some(ship) = state.ships.get(&ship_id) else {
                continue;
            };

            if should_opportunistic_refuel(ship, state, content) {
                continue;
            }

            if deposit_priority(ship, state, content).is_some() {
                continue;
            }

            for priority in &content.autopilot.task_priority {
                let objective = match priority.as_str() {
                    "Mine" => next_mine.next().map(|id| ShipObjective::Mine {
                        asteroid_id: id.clone(),
                    }),
                    "DeepScan" if deep_scan_unlocked => {
                        next_deep_scan.next().map(|id| ShipObjective::DeepScan {
                            asteroid_id: id.clone(),
                        })
                    }
                    "Survey" => next_site.next().map(|id| ShipObjective::Survey {
                        site_id: id.clone(),
                    }),
                    _ => None,
                };
                if let Some(obj) = objective {
                    if let Some(ref mut log) = decisions {
                        let cands = ObjectiveCandidates {
                            mine: &next_mine,
                            site: &next_site,
                            deep_scan: &next_deep_scan,
                        };
                        log_objective_decision(
                            log,
                            state.meta.tick,
                            &self.station_id,
                            &obj,
                            priority,
                            &cands,
                        );
                    }
                    if let Some(agent) = ship_agents.get_mut(&ship_id) {
                        agent.objective = Some(obj);
                    }
                    break;
                }
            }
        }
    }
}
