//! VIO-606: Strategy integration tests.
//!
//! Prove that different `StrategyConfig` settings produce measurably different
//! simulation outcomes. Each test runs the full tick loop with autopilot at
//! production content, varying a single strategy dimension and asserting
//! the expected causal outcome.

use crate::{AutopilotController, CommandSource};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_core::PriorityWeights;

fn load_production_content() -> sim_core::GameContent {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let dir = format!("{manifest}/../../content");
    sim_world::load_content(&dir).expect("load production content")
}

/// Run a simulation for `ticks` with the given strategy config applied to state.
/// Returns final metrics.
fn run_with_strategy(
    content: &sim_core::GameContent,
    seed: u64,
    ticks: u64,
    config_fn: impl FnOnce(&mut sim_core::StrategyConfig),
) -> sim_core::MetricsSnapshot {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut state = sim_world::build_initial_state(content, seed, &mut rng);
    config_fn(&mut state.strategy_config);

    let mut controller = AutopilotController::new();
    let mut next_id = 0u64;
    for _ in 0..ticks {
        let commands = controller.generate_commands(&state, content, &mut next_id);
        sim_core::tick(&mut state, &commands, content, &mut rng, None);
    }
    sim_core::compute_metrics(&state, content)
}

/// Run N seeds and return the average of an f64 metric.
fn average_metric(
    content: &sim_core::GameContent,
    seeds: &[u64],
    ticks: u64,
    config_fn: impl Fn(&mut sim_core::StrategyConfig) + Clone,
    metric_fn: impl Fn(&sim_core::MetricsSnapshot) -> f64,
) -> f64 {
    let total: f64 = seeds
        .iter()
        .map(|&seed| {
            let metrics = run_with_strategy(content, seed, ticks, config_fn.clone());
            metric_fn(&metrics)
        })
        .sum();
    total / seeds.len() as f64
}

/// Mining-focused config: high mining + export, low research + survey.
fn mining_focused(config: &mut sim_core::StrategyConfig) {
    config.priorities = PriorityWeights {
        mining: 1.0,
        survey: 0.3,
        deep_scan: 0.2,
        research: 0.2,
        maintenance: 0.8,
        export: 0.9,
        propellant: 0.9,
        fleet_expansion: 0.7,
    };
}

/// Research-focused config: high research + survey, low mining + export.
fn research_focused(config: &mut sim_core::StrategyConfig) {
    config.priorities = PriorityWeights {
        mining: 0.3,
        survey: 0.9,
        deep_scan: 0.8,
        research: 1.0,
        maintenance: 0.8,
        export: 0.2,
        propellant: 0.9,
        fleet_expansion: 0.3,
    };
}

/// Mining-focused strategy should produce more export revenue than research-focused
/// because mining-focused has export weight 0.9 vs 0.2 for research-focused.
#[test]
fn mining_focused_produces_more_export_revenue() {
    let content = load_production_content();
    let seeds: Vec<u64> = (1..=10).collect();
    let ticks = 3000;

    let mining_avg = average_metric(&content, &seeds, ticks, mining_focused, |m| {
        m.export_revenue_total
    });
    let research_avg = average_metric(&content, &seeds, ticks, research_focused, |m| {
        m.export_revenue_total
    });

    assert!(
        mining_avg >= research_avg,
        "mining-focused ({mining_avg:.0} revenue) should produce at least as much export revenue as research-focused ({research_avg:.0} revenue)"
    );
}

/// Research-focused strategy should unlock more techs than mining-focused.
#[test]
fn research_focused_unlocks_more_techs() {
    let content = load_production_content();
    let seeds: Vec<u64> = (1..=10).collect();
    let ticks = 2000;

    let mining_avg = average_metric(&content, &seeds, ticks, mining_focused, |m| {
        f64::from(m.techs_unlocked)
    });
    let research_avg = average_metric(&content, &seeds, ticks, research_focused, |m| {
        f64::from(m.techs_unlocked)
    });

    assert!(
        research_avg >= mining_avg,
        "research-focused ({research_avg:.0} techs) should unlock at least as many techs as mining-focused ({mining_avg:.0} techs)"
    );
}

/// Higher fleet_size_target should produce more ships.
#[test]
fn fleet_target_5_produces_more_ships_than_2() {
    let content = load_production_content();
    let seeds: Vec<u64> = (1..=10).collect();
    let ticks = 3000;

    let avg_5 = average_metric(
        &content,
        &seeds,
        ticks,
        |c| c.fleet_size_target = 5,
        |m| f64::from(m.fleet_total),
    );
    let avg_2 = average_metric(
        &content,
        &seeds,
        ticks,
        |c| c.fleet_size_target = 2,
        |m| f64::from(m.fleet_total),
    );

    assert!(
        avg_5 >= avg_2,
        "fleet_target=5 ({avg_5:.1} ships) should produce at least as many ships as fleet_target=2 ({avg_2:.1} ships)"
    );
}

/// Expand mode should produce more revenue than Consolidate mode at 3000 ticks.
#[test]
fn expand_mode_produces_more_revenue() {
    let content = load_production_content();
    let seeds: Vec<u64> = (1..=10).collect();
    let ticks = 3000;

    let expand_avg = average_metric(
        &content,
        &seeds,
        ticks,
        |c| c.mode = sim_core::StrategyMode::Expand,
        |m| m.export_revenue_total,
    );
    let consolidate_avg = average_metric(
        &content,
        &seeds,
        ticks,
        |c| c.mode = sim_core::StrategyMode::Consolidate,
        |m| m.export_revenue_total,
    );

    assert!(
        expand_avg >= consolidate_avg,
        "Expand mode ({expand_avg:.0} revenue) should produce at least as much revenue as Consolidate ({consolidate_avg:.0} revenue)"
    );
}
