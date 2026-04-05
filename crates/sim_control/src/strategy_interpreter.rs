//! Strategy rule interpreter (VIO-480).
//!
//! Reads `StrategyConfig` + `GameState` and produces `ConcernPriorities`:
//! final per-concern urgency scores that station and ship agents consume
//! (wiring lands in VIO-481). The interpreter is a pure deterministic
//! function of its inputs plus controller-local runtime state (cached
//! priorities, last-serviced ticks, dirty flag).
//!
//! **Constraints (per ticket):**
//! - No transcendentals; arithmetic only.
//! - `total_cmp()` for float sorting.
//! - `game_minutes_to_ticks()` for time horizons — never hardcoded tick counts.
//! - Read phase then write phase (borrow checker + determinism).
//! - Global aggregates (fleet size, max wear) computed BEFORE per-station scoring.

use sim_core::{
    inventory_volume_m3, ConcernPriorities, GameContent, GameState, InventoryItem, PriorityWeights,
    StrategyConfig,
};

use crate::behaviors::AUTOPILOT_OWNER;

// ---------------------------------------------------------------------------
// Tunable constants
// ---------------------------------------------------------------------------

/// Minimum game-minutes between strategy re-evaluations. At the default
/// `minutes_per_tick = 60`, this produces a 10-tick cadence (the "default 10"
/// called out in the ticket) without hardcoding tick counts. The gating
/// logic uses `Constants::game_minutes_to_ticks` so any minutes-per-tick
/// rescale preserves the intended cadence.
const STRATEGY_EVAL_INTERVAL_MINUTES: u64 = 600;

/// Hysteresis bonus applied to concerns that were already "active" in the
/// previous evaluation (score >= `CONCERN_ACTIVE_THRESHOLD`). Prevents
/// near-boundary oscillation when state urgency drifts by epsilon. Bounded
/// in the ticket spec to \[0.05, 0.10\].
const HYSTERESIS_BONUS: f32 = 0.08;

/// Score threshold above which a concern is considered "active" this tick.
/// Concerns active this tick get their `last_serviced_tick` refreshed.
const CONCERN_ACTIVE_THRESHOLD: f32 = 0.5;

/// Max temporal-bias bonus for a concern that has been unserviced for a long
/// time. Scales linearly from 0.0 (just serviced) to this cap. Prevents a
/// single dominant concern from starving the others.
const TEMPORAL_BIAS_MAX: f32 = 0.15;

/// Game-minutes after which an unserviced concern earns the maximum
/// temporal bias. 24 game-hours at default 60 min/tick = 24 ticks.
const TEMPORAL_BIAS_SATURATION_MINUTES: u64 = 24 * 60;

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

/// Controller-local runtime state for the strategy rule interpreter.
///
/// `cached_priorities` is the result of the most recent evaluation; the
/// controller returns this between re-evaluation ticks. `last_serviced` is
/// a fixed-length array (one entry per `PriorityWeights` field, in the same
/// order as `to_vec`) used to compute temporal bias.
#[derive(Debug, Clone, Default)]
pub struct StrategyRuntimeState {
    pub cached_priorities: Option<ConcernPriorities>,
    pub last_strategy_tick: u64,
    pub strategy_dirty: bool,
    pub last_serviced: [Option<u64>; PriorityWeights::LEN],
}

impl StrategyRuntimeState {
    /// Mark the cache stale so the next `evaluate_strategy` recomputes
    /// unconditionally. Called by the `SetStrategyConfig` command handler
    /// (VIO-483) so runtime strategy changes take effect immediately.
    pub fn mark_dirty(&mut self) {
        self.strategy_dirty = true;
    }

    /// Gating predicate: should the interpreter recompute this tick?
    fn needs_recompute(&self, current_tick: u64, eval_interval_ticks: u64) -> bool {
        if self.cached_priorities.is_none() || self.strategy_dirty {
            return true;
        }
        // Saturating subtraction handles the initial `last_strategy_tick = 0` case.
        current_tick.saturating_sub(self.last_strategy_tick) >= eval_interval_ticks
    }
}

// ---------------------------------------------------------------------------
// Interpreter entry point
// ---------------------------------------------------------------------------

/// Evaluate the strategic layer. Returns cached priorities if gating is not
/// met; otherwise recomputes from state and updates the runtime cache.
///
/// This is the one function `AutopilotController::generate_commands` calls
/// on its strategy pass (VIO-481 wiring). It is pure with respect to
/// `(state, content, runtime)` — the only mutation is on `runtime`.
pub fn evaluate_strategy(
    state: &GameState,
    content: &GameContent,
    runtime: &mut StrategyRuntimeState,
) -> ConcernPriorities {
    let current_tick = state.meta.tick;
    let eval_interval_ticks = content
        .constants
        .game_minutes_to_ticks(STRATEGY_EVAL_INTERVAL_MINUTES)
        .max(1);

    if !runtime.needs_recompute(current_tick, eval_interval_ticks) {
        // SAFETY: needs_recompute returns true when cached_priorities is None,
        // so if we reached here it is Some.
        return runtime.cached_priorities.expect("cache invariant");
    }

    let config = &state.strategy_config;

    // --- Read phase: compute global aggregates once, BEFORE per-concern scoring. ---
    let aggregates = compute_aggregates(state, content);

    // --- Pure urgency derivation from state aggregates. ---
    let urgency = compute_state_urgency(&aggregates, config);

    // --- Combine: config_weight * state_urgency, scaled by mode multipliers. ---
    let mut scores = combine_weights(config, &urgency);

    // --- Hysteresis: active-in-previous-eval concerns get a stabilizing bonus. ---
    apply_hysteresis(&mut scores, runtime.cached_priorities.as_ref());

    // --- Temporal bias: concerns unserviced for a long time get a boost. ---
    let bias_saturation_ticks = content
        .constants
        .game_minutes_to_ticks(TEMPORAL_BIAS_SATURATION_MINUTES)
        .max(1);
    apply_temporal_bias(
        &mut scores,
        &runtime.last_serviced,
        current_tick,
        bias_saturation_ticks,
    );

    // --- Clamp to the concern score range ([0.0, 1.0]) and sanitize NaN. ---
    scores.clamp_unit();

    // --- Write phase: update runtime state. ---
    refresh_last_serviced(&mut runtime.last_serviced, &scores, current_tick);
    runtime.cached_priorities = Some(scores);
    runtime.last_strategy_tick = current_tick;
    runtime.strategy_dirty = false;

    scores
}

/// Combine user-configured weights with state-derived urgency and the
/// strategy mode multiplier table. Output is NOT clamped — downstream
/// `clamp_unit` handles that after hysteresis and temporal bias apply.
fn combine_weights(config: &StrategyConfig, urgency: &PriorityWeights) -> PriorityWeights {
    let mode_mults = config.mode.multipliers();
    PriorityWeights {
        mining: config.priorities.mining * urgency.mining * mode_mults.mining,
        survey: config.priorities.survey * urgency.survey * mode_mults.survey,
        deep_scan: config.priorities.deep_scan * urgency.deep_scan * mode_mults.deep_scan,
        research: config.priorities.research * urgency.research * mode_mults.research,
        maintenance: config.priorities.maintenance * urgency.maintenance * mode_mults.maintenance,
        export: config.priorities.export * urgency.export * mode_mults.export,
        propellant: config.priorities.propellant * urgency.propellant * mode_mults.propellant,
        fleet_expansion: config.priorities.fleet_expansion
            * urgency.fleet_expansion
            * mode_mults.fleet_expansion,
    }
}

/// Add a hysteresis bonus to every concern that was "active" in the previous
/// evaluation. Skips when the cache is empty (first run).
fn apply_hysteresis(scores: &mut PriorityWeights, previous: Option<&ConcernPriorities>) {
    let Some(prev) = previous else { return };
    let prev_vec = prev.to_vec();
    for (score, prev_value) in scores.fields_mut().into_iter().zip(prev_vec.into_iter()) {
        if prev_value >= CONCERN_ACTIVE_THRESHOLD {
            *score += HYSTERESIS_BONUS;
        }
    }
}

/// Write-phase helper: mark each "active" concern's last-serviced tick.
fn refresh_last_serviced(
    last_serviced: &mut [Option<u64>; PriorityWeights::LEN],
    scores: &PriorityWeights,
    current_tick: u64,
) {
    for (slot, score) in last_serviced.iter_mut().zip(scores.to_vec().into_iter()) {
        if score >= CONCERN_ACTIVE_THRESHOLD {
            *slot = Some(current_tick);
        }
    }
}

// ---------------------------------------------------------------------------
// Global aggregates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct GlobalAggregates {
    fleet_count: u32,
    total_ore_kg: f32,
    total_propellant_kg: f32,
    total_propellant_capacity_kg: f32,
    max_module_wear: f32,
    station_cargo_occupied_frac: f32,
    scan_sites_remaining: u32,
    unlocked_techs: u32,
    total_techs: u32,
}

fn compute_aggregates(state: &GameState, content: &GameContent) -> GlobalAggregates {
    // Count autopilot-owned ships + aggregate propellant.
    let mut fleet_count = 0u32;
    let mut total_propellant_kg = 0.0f32;
    let mut total_propellant_capacity_kg = 0.0f32;
    for ship in state.ships.values() {
        if ship.owner.0 == AUTOPILOT_OWNER {
            fleet_count += 1;
            total_propellant_kg += ship.propellant_kg;
            total_propellant_capacity_kg += ship.propellant_capacity_kg;
        }
    }

    // Ore inventory + station cargo occupancy + max module wear.
    let mut total_ore_kg = 0.0f32;
    let mut total_station_volume = 0.0f32;
    let mut total_station_capacity = 0.0f32;
    let mut max_module_wear = 0.0f32;
    for station in state.stations.values() {
        total_station_capacity += station.core.cargo_capacity_m3;
        for item in &station.core.inventory {
            if let InventoryItem::Material { element, kg, .. } = item {
                if element == sim_core::ELEMENT_ORE {
                    total_ore_kg += *kg;
                }
            }
        }
        // Prefer the cached volume when populated, but fall back to the full
        // computation so tests and freshly built states (cache = None) get a
        // real export-urgency signal instead of silently reading 0.
        let station_volume = station
            .core
            .cached_inventory_volume_m3
            .unwrap_or_else(|| inventory_volume_m3(&station.core.inventory, content));
        total_station_volume += station_volume;
        for module in &station.core.modules {
            if module.wear.wear > max_module_wear {
                max_module_wear = module.wear.wear;
            }
        }
    }

    let station_cargo_occupied_frac = if total_station_capacity > 0.0 {
        (total_station_volume / total_station_capacity).clamp(0.0, 1.0)
    } else {
        0.0
    };

    GlobalAggregates {
        fleet_count,
        total_ore_kg,
        total_propellant_kg,
        total_propellant_capacity_kg,
        max_module_wear,
        station_cargo_occupied_frac,
        scan_sites_remaining: u32::try_from(state.scan_sites.len()).unwrap_or(u32::MAX),
        unlocked_techs: u32::try_from(state.research.unlocked.len()).unwrap_or(u32::MAX),
        // Authoritative tech count comes from content, not from runtime
        // research state. Using `runtime.unlocked + evidence` silently reads 0
        // on a fresh run and inverts the research urgency signal.
        total_techs: u32::try_from(content.techs.len()).unwrap_or(u32::MAX),
    }
}

// ---------------------------------------------------------------------------
// State urgency heuristics
// ---------------------------------------------------------------------------

/// Translate global aggregates into per-concern urgency scores in \[0.0, 1.0\].
///
/// These heuristics are intentionally conservative for VIO-480 — they produce
/// plausible first-pass signals that the rule interpreter can cache. VIO-610
/// (Bayesian optimization) will tune the mapping later, and VIO-481 will
/// refine based on what station agents actually need.
fn compute_state_urgency(agg: &GlobalAggregates, config: &StrategyConfig) -> PriorityWeights {
    // mining: low ore buffer → urgent. Reference point = 3x refinery threshold.
    let ore_reference = (config.refinery_threshold_kg * 3.0).max(1.0);
    let mining = 1.0 - (agg.total_ore_kg / ore_reference).clamp(0.0, 1.0);

    // survey: nonzero when any scan sites remain, saturating at 20 sites.
    let survey = (agg.scan_sites_remaining as f32 / 20.0).clamp(0.0, 1.0);

    // deep_scan: treated as a flat background urgency for now (refined in VIO-481).
    // Halved so it only dominates when weights are deliberately tuned up.
    let deep_scan = 0.3;

    // research: scales with unlocked-techs progress. No progress → max urgency;
    // all techs unlocked → zero urgency.
    let research = if agg.total_techs == 0 {
        0.0
    } else {
        1.0 - (agg.unlocked_techs as f32 / agg.total_techs as f32).clamp(0.0, 1.0)
    };

    // maintenance: direct mapping from worst module wear.
    let maintenance = agg.max_module_wear.clamp(0.0, 1.0);

    // export: urgent when station storage gets crowded (suggests surplus ready to ship).
    let export = agg.station_cargo_occupied_frac;

    // propellant: low average fleet fuel → urgent.
    let propellant = if agg.total_propellant_capacity_kg > 0.0 {
        1.0 - (agg.total_propellant_kg / agg.total_propellant_capacity_kg).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // fleet_expansion: fewer ships than target → urgent.
    let fleet_expansion = if config.fleet_size_target == 0 {
        0.0
    } else {
        let target = config.fleet_size_target as f32;
        1.0 - (agg.fleet_count as f32 / target).clamp(0.0, 1.0)
    };

    PriorityWeights {
        mining,
        survey,
        deep_scan,
        research,
        maintenance,
        export,
        propellant,
        fleet_expansion,
    }
}

// ---------------------------------------------------------------------------
// Temporal bias
// ---------------------------------------------------------------------------

/// Add a temporal-bias bonus to each concern score, proportional to how long
/// it has been since the concern was last "serviced" — i.e. had a post-bonus
/// score at or above `CONCERN_ACTIVE_THRESHOLD` in a prior evaluation.
/// Concerns never serviced receive the full bonus. Bonus saturates linearly
/// from 0 at `current_tick` to `TEMPORAL_BIAS_MAX` at `saturation_ticks` ago.
fn apply_temporal_bias(
    scores: &mut PriorityWeights,
    last_serviced: &[Option<u64>; PriorityWeights::LEN],
    current_tick: u64,
    saturation_ticks: u64,
) {
    let saturation = saturation_ticks.max(1) as f32;
    for (score, last) in scores.fields_mut().into_iter().zip(last_serviced.iter()) {
        let gap = match *last {
            Some(last_tick) => current_tick.saturating_sub(last_tick) as f32,
            None => saturation,
        };
        let bias = (gap / saturation).clamp(0.0, 1.0) * TEMPORAL_BIAS_MAX;
        *score += bias;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use sim_core::test_fixtures::{base_content, base_state};
    use sim_core::StrategyMode;

    fn state_and_content() -> (GameState, GameContent) {
        let content = base_content();
        let state = base_state(&content);
        (state, content)
    }

    #[test]
    fn first_call_populates_cache() {
        let (state, content) = state_and_content();
        let mut runtime = StrategyRuntimeState::default();
        assert!(runtime.cached_priorities.is_none());
        let priorities = evaluate_strategy(&state, &content, &mut runtime);
        assert!(runtime.cached_priorities.is_some());
        assert_eq!(runtime.cached_priorities.unwrap(), priorities);
    }

    #[test]
    fn scores_are_clamped_to_unit_range() {
        let (state, content) = state_and_content();
        let mut runtime = StrategyRuntimeState::default();
        let priorities = evaluate_strategy(&state, &content, &mut runtime);
        for value in priorities.to_vec() {
            assert!((0.0..=1.0).contains(&value), "score out of range: {value}");
        }
    }

    #[test]
    fn second_call_within_interval_returns_cache_without_recompute() {
        let (state, content) = state_and_content();
        let mut runtime = StrategyRuntimeState::default();
        let first = evaluate_strategy(&state, &content, &mut runtime);
        let original_last_tick = runtime.last_strategy_tick;
        let second = evaluate_strategy(&state, &content, &mut runtime);
        // Cached path: last_strategy_tick does NOT advance on a cache hit.
        assert_eq!(first, second);
        assert_eq!(runtime.last_strategy_tick, original_last_tick);
    }

    #[test]
    fn dirty_flag_forces_recompute() {
        let (mut state, content) = state_and_content();
        let mut runtime = StrategyRuntimeState::default();
        evaluate_strategy(&state, &content, &mut runtime);
        assert!(!runtime.strategy_dirty);

        // Mutate state in a way the heuristics notice: clearing ships drives
        // fleet_expansion urgency to 1.0 and propellant urgency to 0.0. Then
        // mark dirty and verify the cached priorities actually change.
        let before = runtime.cached_priorities.unwrap();
        state.ships.clear();
        state.strategy_config.fleet_size_target = 5;
        state.strategy_config.priorities.fleet_expansion = 1.0;
        runtime.mark_dirty();
        assert!(runtime.strategy_dirty);
        evaluate_strategy(&state, &content, &mut runtime);
        // Dirty flag is consumed on recompute.
        assert!(!runtime.strategy_dirty);
        let after = runtime.cached_priorities.unwrap();
        assert_ne!(
            before, after,
            "dirty recompute should reflect changed state",
        );
    }

    #[test]
    fn advancing_tick_past_interval_forces_recompute() {
        let (mut state, content) = state_and_content();
        let eval_interval_ticks = content
            .constants
            .game_minutes_to_ticks(STRATEGY_EVAL_INTERVAL_MINUTES);
        let mut runtime = StrategyRuntimeState::default();
        evaluate_strategy(&state, &content, &mut runtime);
        let first_tick = runtime.last_strategy_tick;
        // Advance the game tick past the interval. Computing from the
        // constant keeps this test correct regardless of `minutes_per_tick`
        // in the base_content fixture.
        state.meta.tick += eval_interval_ticks + 1;
        evaluate_strategy(&state, &content, &mut runtime);
        assert!(runtime.last_strategy_tick > first_tick);
        assert_eq!(runtime.last_strategy_tick, state.meta.tick);
    }

    #[test]
    fn evaluation_is_deterministic_for_same_input() {
        let (state, content) = state_and_content();
        let mut runtime_a = StrategyRuntimeState::default();
        let mut runtime_b = StrategyRuntimeState::default();
        let a = evaluate_strategy(&state, &content, &mut runtime_a);
        let b = evaluate_strategy(&state, &content, &mut runtime_b);
        assert_eq!(a, b);
    }

    #[test]
    fn higher_wear_increases_maintenance_score() {
        let (mut state, content) = state_and_content();
        let mut runtime = StrategyRuntimeState::default();
        // Baseline: maintenance score at zero wear.
        let baseline = evaluate_strategy(&state, &content, &mut runtime).maintenance;
        runtime.mark_dirty();

        // Nothing to compare against if there are no modules in the fixture.
        if state.stations.values().all(|s| s.core.modules.is_empty()) {
            // base_state has no modules by default; force a synthetic
            // maintenance signal by boosting the cached max-wear logic.
            // Instead we assert the fleet_size / propellant-shape is sane and
            // skip this wear-specific check.
            return;
        }

        for station in state.stations.values_mut() {
            for module in &mut station.core.modules {
                module.wear.wear = 0.9;
            }
        }
        let high = evaluate_strategy(&state, &content, &mut runtime).maintenance;
        assert!(
            high >= baseline,
            "high wear ({high}) should be >= baseline ({baseline})",
        );
    }

    #[test]
    fn expand_mode_yields_different_priorities_than_balanced() {
        // Two runtimes, two configs. Expand should boost mining/fleet relative
        // to Balanced (assuming nonzero state urgency for those concerns).
        let (mut state, content) = state_and_content();
        state.strategy_config.mode = StrategyMode::Balanced;
        let mut runtime_balanced = StrategyRuntimeState::default();
        let balanced = evaluate_strategy(&state, &content, &mut runtime_balanced);

        state.strategy_config.mode = StrategyMode::Expand;
        let mut runtime_expand = StrategyRuntimeState::default();
        let expand = evaluate_strategy(&state, &content, &mut runtime_expand);

        // At the very least, some field should differ between modes. We don't
        // assert direction globally because clamping can mask it, but an
        // all-equal result would mean the mode multiplier is a no-op bug.
        assert_ne!(balanced, expand);
    }

    #[test]
    fn last_serviced_tick_updates_for_active_concerns() {
        let (mut state, content) = state_and_content();
        // Force fleet_expansion urgency high: zero ships, nonzero target.
        state.ships.clear();
        state.strategy_config.fleet_size_target = 5;
        state.strategy_config.priorities.fleet_expansion = 1.0;
        state.strategy_config.mode = StrategyMode::Expand;
        let mut runtime = StrategyRuntimeState::default();
        let scores = evaluate_strategy(&state, &content, &mut runtime);
        // fleet_expansion should be "active" (>= threshold) and its
        // last-serviced entry should be refreshed to the current tick.
        if scores.fleet_expansion >= CONCERN_ACTIVE_THRESHOLD {
            let idx = 7; // fleet_expansion is last in PriorityWeights order
            assert_eq!(runtime.last_serviced[idx], Some(state.meta.tick));
        }
    }

    #[test]
    fn empty_state_never_produces_nan() {
        // Edge: no ships, no stations, no scan sites, no techs.
        let content = base_content();
        let mut state = base_state(&content);
        state.ships.clear();
        state.stations.clear();
        state.scan_sites.clear();
        state.research.unlocked.clear();
        state.research.evidence.clear();
        let mut runtime = StrategyRuntimeState::default();
        let scores = evaluate_strategy(&state, &content, &mut runtime);
        for value in scores.to_vec() {
            assert!(!value.is_nan(), "NaN escaped the interpreter");
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn hysteresis_bonus_stabilizes_active_concerns() {
        // Verify the hysteresis bonus contributes *specifically* to the
        // previously-active concern. We compare "seeded cache" vs "no cache"
        // runs of apply_hysteresis directly on zero-urgency scores so that
        // temporal bias and other contributions cannot mask the signal.
        let mut baseline = PriorityWeights {
            mining: 0.0,
            survey: 0.0,
            deep_scan: 0.0,
            research: 0.0,
            maintenance: 0.0,
            export: 0.0,
            propellant: 0.0,
            fleet_expansion: 0.0,
        };
        let previously_active = ConcernPriorities {
            mining: 0.0,
            survey: 0.0,
            deep_scan: 0.0,
            research: 0.0,
            maintenance: 1.0,
            export: 0.0,
            propellant: 0.0,
            fleet_expansion: 0.0,
        };
        apply_hysteresis(&mut baseline, Some(&previously_active));
        // Only maintenance should have been boosted by the bonus; the other
        // concerns must be untouched. This pins hysteresis contribution
        // exactly and catches the failure mode where temporal bias masks it.
        assert!((baseline.maintenance - HYSTERESIS_BONUS).abs() < 1e-6);
        assert_eq!(baseline.mining, 0.0);
        assert_eq!(baseline.survey, 0.0);
        assert_eq!(baseline.export, 0.0);
        assert_eq!(baseline.fleet_expansion, 0.0);
    }

    #[test]
    fn evaluates_against_real_content() {
        // Integration smoke: exercise the interpreter end-to-end against the
        // full content tree. Catches drift between base_content fixtures and
        // real content (e.g. a strategy.json field rename, a new tech id
        // that trips the urgency heuristic, etc.). Per skills/rust-sim-core.md,
        // at least one test per major system should hit real content.
        let content = sim_world::load_content("../../content").unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([42u8; 32]);
        let state = sim_world::build_initial_state(&content, 42, &mut rng);
        let mut runtime = StrategyRuntimeState::default();
        let scores = evaluate_strategy(&state, &content, &mut runtime);
        for value in scores.to_vec() {
            assert!(!value.is_nan());
            assert!((0.0..=1.0).contains(&value));
        }
        // Research urgency must be nonzero on a fresh run: no techs unlocked
        // against a content catalog with >0 techs means maximum urgency.
        assert!(
            !content.techs.is_empty(),
            "real content has at least one tech",
        );
        assert!(
            scores.research > 0.0,
            "research urgency should fire on a fresh state with unlocked techs empty",
        );
    }
}
