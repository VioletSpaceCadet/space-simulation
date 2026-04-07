use crate::alerts::AlertDetail;
use serde::Serialize;
use sim_core::{MetricsSnapshot, RunScore};
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct AdvisorDigest {
    pub tick: u64,
    pub snapshot: MetricsSnapshot,
    pub trends: Vec<TrendInfo>,
    pub rates: Rates,
    pub bottleneck: Bottleneck,
    pub alerts: Vec<AlertDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perf: Option<PerfSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<RunScore>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_trend: Option<TrendDirection>,
    /// VIO-612: Strategy context for the advisor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<StrategyContext>,
}

/// VIO-612: Current strategy mode, game phase, and priority weights.
#[derive(Debug, Clone, Serialize)]
pub struct StrategyContext {
    pub mode: String,
    pub phase: String,
    pub fleet_size_target: u32,
    pub priorities: std::collections::BTreeMap<String, f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerfSummary {
    pub sample_count: usize,
    pub steps: Vec<PerfStepEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerfStepEntry {
    pub name: String,
    pub mean_us: f64,
    pub p95_us: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendInfo {
    pub metric: String,
    pub direction: TrendDirection,
    pub short_avg: f64,
    pub long_avg: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum TrendDirection {
    Improving,
    Declining,
    Stable,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rates {
    pub material_production: f64,
    pub ore_consumption: f64,
    pub wear_accumulation: f64,
    pub slag_accumulation: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Bottleneck {
    OreSupply,
    StorageFull,
    SlagBackpressure,
    WearCritical,
    FleetIdle,
    ResearchStalled,
    Healthy,
}

// ---------------------------------------------------------------------------
// Tracked metric configuration
// ---------------------------------------------------------------------------

struct TrackedMetric {
    name: &'static str,
    extract: fn(&MetricsSnapshot) -> f64,
    higher_is_better: bool,
}

const TRACKED_METRICS: &[TrackedMetric] = &[
    TrackedMetric {
        name: "total_material_kg",
        extract: |s| f64::from(s.total_material_kg),
        higher_is_better: true,
    },
    TrackedMetric {
        name: "total_ore_kg",
        extract: |s| f64::from(s.total_ore_kg),
        higher_is_better: true,
    },
    TrackedMetric {
        name: "total_slag_kg",
        extract: |s| f64::from(s.total_slag_kg),
        higher_is_better: false,
    },
    TrackedMetric {
        name: "avg_module_wear",
        extract: |s| f64::from(s.avg_module_wear),
        higher_is_better: false,
    },
    TrackedMetric {
        name: "balance",
        extract: |s| s.balance,
        higher_is_better: true,
    },
    TrackedMetric {
        name: "total_scan_data",
        extract: |s| f64::from(s.total_scan_data),
        higher_is_better: true,
    },
    TrackedMetric {
        name: "asteroids_discovered",
        extract: |s| f64::from(s.asteroids_discovered),
        higher_is_better: true,
    },
    TrackedMetric {
        name: "station_max_temp_mk",
        extract: |s| f64::from(s.station_max_temp_mk),
        higher_is_better: false,
    },
    TrackedMetric {
        name: "station_avg_temp_mk",
        extract: |s| f64::from(s.station_avg_temp_mk),
        higher_is_better: false,
    },
    TrackedMetric {
        name: "overheat_warning_count",
        extract: |s| f64::from(s.overheat_warning_count),
        higher_is_better: false,
    },
    TrackedMetric {
        name: "overheat_critical_count",
        extract: |s| f64::from(s.overheat_critical_count),
        higher_is_better: false,
    },
    TrackedMetric {
        name: "heat_wear_multiplier_avg",
        extract: |s| f64::from(s.heat_wear_multiplier_avg),
        higher_is_better: false,
    },
    TrackedMetric {
        name: "transfer_volume_kg",
        extract: |s| f64::from(s.transfer_volume_kg),
        higher_is_better: true,
    },
];

const SHORT_WINDOW: usize = 10;
const LONG_WINDOW: usize = 50;

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

fn compute_trends(history: &VecDeque<MetricsSnapshot>) -> Vec<TrendInfo> {
    TRACKED_METRICS
        .iter()
        .map(|metric| {
            let short_avg = window_average(history, SHORT_WINDOW, metric.extract);
            let long_avg = window_average(history, LONG_WINDOW, metric.extract);

            let direction = if long_avg == 0.0 && short_avg == 0.0 {
                TrendDirection::Stable
            } else if short_avg > long_avg * 1.05 {
                if metric.higher_is_better {
                    TrendDirection::Improving
                } else {
                    TrendDirection::Declining
                }
            } else if short_avg < long_avg * 0.95 {
                if metric.higher_is_better {
                    TrendDirection::Declining
                } else {
                    TrendDirection::Improving
                }
            } else {
                TrendDirection::Stable
            };

            TrendInfo {
                metric: metric.name.to_string(),
                direction,
                short_avg,
                long_avg,
            }
        })
        .collect()
}

fn window_average(
    history: &VecDeque<MetricsSnapshot>,
    window: usize,
    extract: fn(&MetricsSnapshot) -> f64,
) -> f64 {
    let count = history.len().min(window);
    if count == 0 {
        return 0.0;
    }
    let sum: f64 = history.iter().rev().take(count).map(extract).sum();
    sum / count as f64
}

fn compute_rates(history: &VecDeque<MetricsSnapshot>) -> Rates {
    if history.len() < 2 {
        return Rates {
            material_production: 0.0,
            ore_consumption: 0.0,
            wear_accumulation: 0.0,
            slag_accumulation: 0.0,
        };
    }
    let last = &history[history.len() - 1];
    let prev = &history[history.len() - 2];

    Rates {
        material_production: f64::from(last.total_material_kg) - f64::from(prev.total_material_kg),
        ore_consumption: f64::from(prev.total_ore_kg) - f64::from(last.total_ore_kg),
        wear_accumulation: f64::from(last.avg_module_wear) - f64::from(prev.avg_module_wear),
        slag_accumulation: f64::from(last.total_slag_kg) - f64::from(prev.total_slag_kg),
    }
}

fn detect_bottleneck(
    history: &VecDeque<MetricsSnapshot>,
    constants: &sim_core::Constants,
) -> Bottleneck {
    let Some(latest) = history.back() else {
        return Bottleneck::Healthy;
    };

    if latest
        .per_module_metrics
        .get("processor")
        .is_some_and(|m| m.starved > 0)
    {
        return Bottleneck::OreSupply;
    }
    if latest.station_storage_used_pct > constants.bottleneck_storage_threshold_pct {
        return Bottleneck::StorageFull;
    }
    if latest.total_slag_kg > latest.total_material_kg * constants.bottleneck_slag_ratio_threshold {
        return Bottleneck::SlagBackpressure;
    }
    if latest.max_module_wear > constants.bottleneck_wear_threshold {
        return Bottleneck::WearCritical;
    }
    if latest.fleet_idle > 0 && latest.fleet_total > 1 {
        return Bottleneck::FleetIdle;
    }
    if latest.total_scan_data < 1.0 && latest.techs_unlocked == 0 {
        return Bottleneck::ResearchStalled;
    }

    Bottleneck::Healthy
}

pub fn compute_digest(
    history: &VecDeque<MetricsSnapshot>,
    alerts: Vec<AlertDetail>,
    timings: &VecDeque<sim_core::TickTimings>,
    constants: &sim_core::Constants,
    score_history: &VecDeque<RunScore>,
    state: Option<&sim_core::GameState>,
) -> Option<AdvisorDigest> {
    let latest = history.back()?;

    let perf = if timings.is_empty() {
        None
    } else {
        Some(compute_perf_summary(timings))
    };

    let (score, score_trend) = if let Some(latest_score) = score_history.back() {
        let trend = compute_score_trend(score_history);
        (Some(latest_score.clone()), Some(trend))
    } else {
        (None, None)
    };

    let strategy = state.map(|s| {
        let priorities = s.strategy_config.priorities;
        StrategyContext {
            mode: format!("{:?}", s.strategy_config.mode),
            phase: format!("{:?}", s.progression.phase),
            fleet_size_target: s.strategy_config.fleet_size_target,
            priorities: [
                ("mining".to_string(), priorities.mining),
                ("survey".to_string(), priorities.survey),
                ("deep_scan".to_string(), priorities.deep_scan),
                ("research".to_string(), priorities.research),
                ("maintenance".to_string(), priorities.maintenance),
                ("export".to_string(), priorities.export),
                ("propellant".to_string(), priorities.propellant),
                ("fleet_expansion".to_string(), priorities.fleet_expansion),
            ]
            .into_iter()
            .collect(),
        }
    });

    Some(AdvisorDigest {
        tick: latest.tick,
        snapshot: latest.clone(),
        trends: compute_trends(history),
        rates: compute_rates(history),
        bottleneck: detect_bottleneck(history, constants),
        alerts,
        perf,
        score,
        score_trend,
        strategy,
    })
}

fn compute_score_trend(score_history: &VecDeque<RunScore>) -> TrendDirection {
    let short_avg = score_window_average(score_history, SHORT_WINDOW);
    let long_avg = score_window_average(score_history, LONG_WINDOW);

    if long_avg == 0.0 && short_avg == 0.0 {
        TrendDirection::Stable
    } else if short_avg > long_avg * 1.05 {
        TrendDirection::Improving
    } else if short_avg < long_avg * 0.95 {
        TrendDirection::Declining
    } else {
        TrendDirection::Stable
    }
}

fn score_window_average(history: &VecDeque<RunScore>, window: usize) -> f64 {
    let count = history.len().min(window);
    if count == 0 {
        return 0.0;
    }
    let sum: f64 = history.iter().rev().take(count).map(|s| s.composite).sum();
    sum / count as f64
}

fn compute_perf_summary(timings: &VecDeque<sim_core::TickTimings>) -> PerfSummary {
    let slice: Vec<_> = timings.iter().cloned().collect();
    let stats = sim_core::compute_step_stats(&slice);
    PerfSummary {
        sample_count: timings.len(),
        steps: stats
            .into_iter()
            .map(|s| PerfStepEntry {
                name: s.name,
                mean_us: s.mean_us,
                p95_us: s.p95_us,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::test_fixtures::base_content;

    fn test_constants() -> sim_core::Constants {
        base_content().constants
    }

    fn empty_snapshot(tick: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            tick,
            metrics_version: 1,
            total_ore_kg: 0.0,
            total_material_kg: 0.0,
            total_slag_kg: 0.0,
            per_element_material_kg: std::collections::BTreeMap::new(),
            station_storage_used_pct: 0.0,
            ship_cargo_used_pct: 0.0,
            per_element_ore_stats: std::collections::BTreeMap::new(),
            ore_lot_count: 0,
            avg_material_quality: 0.0,
            per_module_metrics: std::collections::BTreeMap::new(),
            fleet_total: 0,
            fleet_idle: 0,
            fleet_mining: 0,
            fleet_transiting: 0,
            fleet_surveying: 0,
            fleet_depositing: 0,
            fleet_refueling: 0,
            fleet_propellant_kg: 0.0,
            fleet_propellant_pct: 0.0,
            propellant_consumed_total: 0.0,
            scan_sites_remaining: 0,
            asteroids_discovered: 0,
            asteroids_depleted: 0,
            techs_unlocked: 0,
            total_scan_data: 0.0,
            max_tech_evidence: 0.0,
            avg_module_wear: 0.0,
            max_module_wear: 0.0,
            repair_kits_remaining: 0,
            balance: 0.0,
            crew_salary_per_hour: 0.0,
            thruster_count: 0,
            export_revenue_total: 0.0,
            export_count: 0,
            power_generated_kw: 0.0,
            power_consumed_kw: 0.0,
            power_deficit_kw: 0.0,
            battery_charge_pct: 0.0,
            station_max_temp_mk: 0,
            station_avg_temp_mk: 0,
            overheat_warning_count: 0,
            overheat_critical_count: 0,
            heat_wear_multiplier_avg: 0.0,
            satellites_active: 0,
            satellites_failed: 0,
            transfer_volume_kg: 0.0,
            transfer_count: 0,
            milestones_completed: 0,
            game_phase: 0,
        }
    }

    #[test]
    fn empty_history_returns_none() {
        let history = VecDeque::new();
        assert!(compute_digest(
            &history,
            vec![],
            &VecDeque::new(),
            &test_constants(),
            &VecDeque::new(),
            None,
        )
        .is_none());
    }

    #[test]
    fn single_sample_returns_stable_trends() {
        let mut history = VecDeque::new();
        history.push_back(empty_snapshot(1));

        let digest = compute_digest(
            &history,
            vec![],
            &VecDeque::new(),
            &test_constants(),
            &VecDeque::new(),
            None,
        )
        .unwrap();
        for trend in &digest.trends {
            assert_eq!(
                trend.direction,
                TrendDirection::Stable,
                "metric {} should be Stable with one sample, got {:?}",
                trend.metric,
                trend.direction
            );
        }
    }

    #[test]
    fn increasing_material_is_improving() {
        let mut history = VecDeque::new();
        for tick in 0..50 {
            let mut snap = empty_snapshot(tick);
            snap.total_material_kg = tick as f32 * 10.0;
            history.push_back(snap);
        }

        let trends = compute_trends(&history);
        let material_trend = trends
            .iter()
            .find(|t| t.metric == "total_material_kg")
            .unwrap();
        assert_eq!(material_trend.direction, TrendDirection::Improving);
        assert!(
            material_trend.short_avg > material_trend.long_avg,
            "short_avg ({}) should exceed long_avg ({})",
            material_trend.short_avg,
            material_trend.long_avg
        );
    }

    #[test]
    fn increasing_slag_is_declining() {
        let mut history = VecDeque::new();
        for tick in 0..50 {
            let mut snap = empty_snapshot(tick);
            snap.total_slag_kg = tick as f32 * 10.0;
            history.push_back(snap);
        }

        let trends = compute_trends(&history);
        let slag_trend = trends.iter().find(|t| t.metric == "total_slag_kg").unwrap();
        assert_eq!(slag_trend.direction, TrendDirection::Declining);
    }

    #[test]
    fn rates_compute_delta_between_last_two() {
        let mut history = VecDeque::new();

        let mut prev = empty_snapshot(0);
        prev.total_material_kg = 100.0;
        prev.total_ore_kg = 500.0;
        prev.avg_module_wear = 0.1;
        prev.total_slag_kg = 50.0;
        history.push_back(prev);

        let mut last = empty_snapshot(1);
        last.total_material_kg = 120.0;
        last.total_ore_kg = 480.0;
        last.avg_module_wear = 0.12;
        last.total_slag_kg = 55.0;
        history.push_back(last);

        let rates = compute_rates(&history);
        assert!((rates.material_production - 20.0).abs() < 1e-5);
        assert!((rates.ore_consumption - 20.0).abs() < 1e-5);
        assert!((rates.wear_accumulation - 0.02).abs() < 1e-5);
        assert!((rates.slag_accumulation - 5.0).abs() < 1e-5);
    }

    #[test]
    fn bottleneck_priority_ore_first() {
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        // Set multiple conditions: OreSupply + StorageFull + WearCritical
        snap.per_module_metrics
            .entry("processor".to_string())
            .or_default()
            .starved = 2;
        snap.station_storage_used_pct = 0.98;
        snap.max_module_wear = 0.9;
        history.push_back(snap);

        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::OreSupply
        );
    }

    #[test]
    fn bottleneck_healthy_when_no_issues() {
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        // Set some scan data and techs to avoid ResearchStalled
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);

        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::Healthy
        );
    }

    #[test]
    fn detect_bottleneck_each_type() {
        // OreSupply
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.per_module_metrics
            .entry("processor".to_string())
            .or_default()
            .starved = 1;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::OreSupply
        );

        // StorageFull
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.station_storage_used_pct = 0.96;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::StorageFull
        );

        // SlagBackpressure
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.total_slag_kg = 100.0;
        snap.total_material_kg = 100.0; // slag > material * 0.5
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::SlagBackpressure
        );

        // WearCritical
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.max_module_wear = 0.85;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::WearCritical
        );

        // FleetIdle
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.fleet_idle = 1;
        snap.fleet_total = 3;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::FleetIdle
        );

        // ResearchStalled
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.total_scan_data = 0.5;
        snap.techs_unlocked = 0;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &test_constants()),
            Bottleneck::ResearchStalled
        );
    }

    #[test]
    fn custom_thresholds_change_detection() {
        let mut constants = test_constants();

        // With default thresholds (0.95), 0.96 triggers StorageFull
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.station_storage_used_pct = 0.96;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &constants),
            Bottleneck::StorageFull
        );

        // Raise threshold to 0.99 — same value no longer triggers
        constants.bottleneck_storage_threshold_pct = 0.99;
        assert_eq!(detect_bottleneck(&history, &constants), Bottleneck::Healthy);

        // Custom slag ratio: lower threshold triggers on less slag
        constants.bottleneck_slag_ratio_threshold = 0.3;
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.total_slag_kg = 40.0;
        snap.total_material_kg = 100.0; // 40 > 100*0.3 but 40 < 100*0.5
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(
            detect_bottleneck(&history, &constants),
            Bottleneck::SlagBackpressure
        );

        // Custom wear threshold: higher threshold avoids trigger
        constants.bottleneck_slag_ratio_threshold = 0.5; // reset
        constants.bottleneck_wear_threshold = 0.95;
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.max_module_wear = 0.9; // below 0.95
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(detect_bottleneck(&history, &constants), Bottleneck::Healthy);
    }

    #[test]
    fn exact_threshold_values_do_not_trigger() {
        let constants = test_constants();

        // Value exactly at threshold should NOT trigger (strict > comparison)
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.station_storage_used_pct = constants.bottleneck_storage_threshold_pct; // exactly 0.95
        snap.total_slag_kg = constants.bottleneck_slag_ratio_threshold * 100.0; // exactly at ratio
        snap.total_material_kg = 100.0;
        snap.max_module_wear = constants.bottleneck_wear_threshold; // exactly 0.8
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);

        assert_eq!(detect_bottleneck(&history, &constants), Bottleneck::Healthy);
    }

    #[test]
    fn score_trend_improving_when_increasing() {
        let mut score_history = VecDeque::new();
        for tick in 0..50u64 {
            score_history.push_back(RunScore {
                composite: tick as f64 * 10.0,
                threshold: "Startup".to_string(),
                tick,
                dimensions: std::collections::BTreeMap::new(),
            });
        }
        assert_eq!(
            compute_score_trend(&score_history),
            TrendDirection::Improving
        );
    }

    #[test]
    fn score_trend_declining_when_decreasing() {
        let mut score_history = VecDeque::new();
        for tick in 0..50u64 {
            score_history.push_back(RunScore {
                composite: (50 - tick) as f64 * 10.0,
                threshold: "Startup".to_string(),
                tick,
                dimensions: std::collections::BTreeMap::new(),
            });
        }
        assert_eq!(
            compute_score_trend(&score_history),
            TrendDirection::Declining
        );
    }

    #[test]
    fn score_trend_stable_with_single_entry() {
        let mut score_history = VecDeque::new();
        score_history.push_back(RunScore {
            composite: 100.0,
            threshold: "Startup".to_string(),
            tick: 1,
            dimensions: std::collections::BTreeMap::new(),
        });
        assert_eq!(compute_score_trend(&score_history), TrendDirection::Stable);
    }

    #[test]
    fn digest_includes_score_when_present() {
        let mut history = VecDeque::new();
        history.push_back(empty_snapshot(1));

        let mut score_history = VecDeque::new();
        score_history.push_back(RunScore {
            composite: 250.0,
            threshold: "Contractor".to_string(),
            tick: 1,
            dimensions: std::collections::BTreeMap::new(),
        });

        let digest = compute_digest(
            &history,
            vec![],
            &VecDeque::new(),
            &test_constants(),
            &score_history,
            None,
        )
        .unwrap();
        assert!(digest.score.is_some());
        assert!((digest.score.unwrap().composite - 250.0).abs() < 1e-6);
        assert_eq!(digest.score_trend.unwrap(), TrendDirection::Stable);
    }

    #[test]
    fn digest_omits_score_when_empty() {
        let mut history = VecDeque::new();
        history.push_back(empty_snapshot(1));

        let digest = compute_digest(
            &history,
            vec![],
            &VecDeque::new(),
            &test_constants(),
            &VecDeque::new(),
            None,
        )
        .unwrap();
        assert!(digest.score.is_none());
        assert!(digest.score_trend.is_none());
    }
}
