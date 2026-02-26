use serde::Serialize;
use sim_core::MetricsSnapshot;
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
    pub material_production_per_sample: f64,
    pub ore_consumption_per_sample: f64,
    pub wear_accumulation_per_sample: f64,
    pub slag_accumulation_per_sample: f64,
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

#[derive(Debug, Clone, Serialize)]
pub struct AlertDetail {
    pub id: String,
    pub severity: String,
    pub message: String,
    pub suggested_action: String,
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
            material_production_per_sample: 0.0,
            ore_consumption_per_sample: 0.0,
            wear_accumulation_per_sample: 0.0,
            slag_accumulation_per_sample: 0.0,
        };
    }
    let last = &history[history.len() - 1];
    let prev = &history[history.len() - 2];

    Rates {
        material_production_per_sample: f64::from(last.total_material_kg)
            - f64::from(prev.total_material_kg),
        ore_consumption_per_sample: f64::from(prev.total_ore_kg) - f64::from(last.total_ore_kg),
        wear_accumulation_per_sample: f64::from(last.avg_module_wear)
            - f64::from(prev.avg_module_wear),
        slag_accumulation_per_sample: f64::from(last.total_slag_kg) - f64::from(prev.total_slag_kg),
    }
}

fn detect_bottleneck(history: &VecDeque<MetricsSnapshot>) -> Bottleneck {
    let Some(latest) = history.back() else {
        return Bottleneck::Healthy;
    };

    if latest.refinery_starved_count > 0 {
        return Bottleneck::OreSupply;
    }
    if latest.station_storage_used_pct > 0.95 {
        return Bottleneck::StorageFull;
    }
    if latest.total_slag_kg > latest.total_material_kg * 0.5 {
        return Bottleneck::SlagBackpressure;
    }
    if latest.max_module_wear > 0.8 {
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
) -> Option<AdvisorDigest> {
    let latest = history.back()?;

    Some(AdvisorDigest {
        tick: latest.tick,
        snapshot: latest.clone(),
        trends: compute_trends(history),
        rates: compute_rates(history),
        bottleneck: detect_bottleneck(history),
        alerts,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_snapshot(tick: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            tick,
            metrics_version: 1,
            total_ore_kg: 0.0,
            total_material_kg: 0.0,
            total_slag_kg: 0.0,
            total_iron_material_kg: 0.0,
            station_storage_used_pct: 0.0,
            ship_cargo_used_pct: 0.0,
            avg_ore_fe_fraction: 0.0,
            ore_lot_count: 0,
            min_ore_fe_fraction: 0.0,
            max_ore_fe_fraction: 0.0,
            avg_material_quality: 0.0,
            refinery_active_count: 0,
            refinery_starved_count: 0,
            refinery_stalled_count: 0,
            assembler_active_count: 0,
            assembler_stalled_count: 0,
            fleet_total: 0,
            fleet_idle: 0,
            fleet_mining: 0,
            fleet_transiting: 0,
            fleet_surveying: 0,
            fleet_depositing: 0,
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
            thruster_count: 0,
        }
    }

    #[test]
    fn empty_history_returns_none() {
        let history = VecDeque::new();
        assert!(compute_digest(&history, vec![]).is_none());
    }

    #[test]
    fn single_sample_returns_stable_trends() {
        let mut history = VecDeque::new();
        history.push_back(empty_snapshot(1));

        let digest = compute_digest(&history, vec![]).unwrap();
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
        assert!((rates.material_production_per_sample - 20.0).abs() < 1e-5);
        assert!((rates.ore_consumption_per_sample - 20.0).abs() < 1e-5);
        assert!((rates.wear_accumulation_per_sample - 0.02).abs() < 1e-5);
        assert!((rates.slag_accumulation_per_sample - 5.0).abs() < 1e-5);
    }

    #[test]
    fn bottleneck_priority_ore_first() {
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        // Set multiple conditions: OreSupply + StorageFull + WearCritical
        snap.refinery_starved_count = 2;
        snap.station_storage_used_pct = 0.98;
        snap.max_module_wear = 0.9;
        history.push_back(snap);

        assert_eq!(detect_bottleneck(&history), Bottleneck::OreSupply);
    }

    #[test]
    fn bottleneck_healthy_when_no_issues() {
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        // Set some scan data and techs to avoid ResearchStalled
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);

        assert_eq!(detect_bottleneck(&history), Bottleneck::Healthy);
    }

    #[test]
    fn detect_bottleneck_each_type() {
        // OreSupply
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.refinery_starved_count = 1;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(detect_bottleneck(&history), Bottleneck::OreSupply);

        // StorageFull
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.station_storage_used_pct = 0.96;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(detect_bottleneck(&history), Bottleneck::StorageFull);

        // SlagBackpressure
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.total_slag_kg = 100.0;
        snap.total_material_kg = 100.0; // slag > material * 0.5
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(detect_bottleneck(&history), Bottleneck::SlagBackpressure);

        // WearCritical
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.max_module_wear = 0.85;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(detect_bottleneck(&history), Bottleneck::WearCritical);

        // FleetIdle
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.fleet_idle = 1;
        snap.fleet_total = 3;
        snap.total_scan_data = 10.0;
        snap.techs_unlocked = 1;
        history.push_back(snap);
        assert_eq!(detect_bottleneck(&history), Bottleneck::FleetIdle);

        // ResearchStalled
        let mut history = VecDeque::new();
        let mut snap = empty_snapshot(1);
        snap.total_scan_data = 0.5;
        snap.techs_unlocked = 0;
        history.push_back(snap);
        assert_eq!(detect_bottleneck(&history), Bottleneck::ResearchStalled);
    }
}
