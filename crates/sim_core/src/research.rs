use rand::Rng;
use crate::{Event, EventLevel, GameContent, GameState, StationId, TechId};

pub(crate) fn advance_research(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Sort station IDs for deterministic RNG consumption order.
    let mut station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    station_ids.sort_by(|a, b| a.0.cmp(&b.0));

    for station_id in station_ids {
        let (compute_total, power_per_unit, efficiency) = {
            let facilities = &state.stations[&station_id].facilities;
            (
                facilities.compute_units_total,
                facilities.power_per_compute_unit_per_tick,
                facilities.efficiency,
            )
        };

        // Collect eligible techs: prereqs met, not yet unlocked. Sort for determinism.
        let mut eligible: Vec<(TechId, f32)> = content
            .techs
            .iter()
            .filter(|tech| {
                !state.research.unlocked.contains(&tech.id)
                    && tech
                        .prereqs
                        .iter()
                        .all(|prereq| state.research.unlocked.contains(prereq))
            })
            .map(|tech| (tech.id.clone(), tech.difficulty))
            .collect();
        eligible.sort_by(|(a, _), (b, _)| a.0.cmp(&b.0));

        if eligible.is_empty() {
            continue;
        }

        let per_tech_compute = compute_total as f32 / eligible.len() as f32;
        let total_power = compute_total as f32 * power_per_unit;

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::PowerConsumed {
                station_id: station_id.clone(),
                amount: total_power,
            },
        ));

        for (tech_id, difficulty) in eligible {
            let current_evidence = {
                let evidence = state
                    .research
                    .evidence
                    .entry(tech_id.clone())
                    .or_insert(0.0);
                *evidence += per_tech_compute * efficiency;
                *evidence
            };

            let p = 1.0 - (-current_evidence / difficulty).exp();
            let rolled: f32 = rng.gen();

            if event_level == EventLevel::Debug {
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ResearchRoll {
                        tech_id: tech_id.clone(),
                        evidence: current_evidence,
                        p,
                        rolled,
                    },
                ));
            }

            if rolled < p {
                state.research.unlocked.insert(tech_id.clone());
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::TechUnlocked { tech_id },
                ));
            }
        }
    }
}
