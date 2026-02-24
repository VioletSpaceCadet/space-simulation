use serde::Serialize;
use sim_core::MetricsSnapshot;

type Extractor = (&'static str, Box<dyn Fn(&MetricsSnapshot) -> f64>);

#[derive(Debug, Serialize)]
pub struct SummaryStats {
    pub seed_count: usize,
    pub collapsed_count: usize,
    pub metrics: Vec<MetricSummary>,
}

#[derive(Debug, Serialize)]
pub struct MetricSummary {
    pub name: String,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub stddev: f64,
}

pub fn compute_summary(snapshots: &[(u64, &MetricsSnapshot)]) -> SummaryStats {
    let seed_count = snapshots.len();

    // A seed is "collapsed" if final snapshot has refinery_starved_count > 0 AND fleet_idle == fleet_total
    let collapsed_count = snapshots
        .iter()
        .filter(|(_, s)| s.refinery_starved_count > 0 && s.fleet_idle == s.fleet_total)
        .count();

    let extractors: Vec<Extractor> = vec![
        (
            "storage_saturation_pct",
            Box::new(|s| f64::from(s.station_storage_used_pct)),
        ),
        (
            "fleet_idle_pct",
            Box::new(|s| {
                if s.fleet_total == 0 {
                    0.0
                } else {
                    f64::from(s.fleet_idle) / f64::from(s.fleet_total)
                }
            }),
        ),
        (
            "refinery_starved_count",
            Box::new(|s| f64::from(s.refinery_starved_count)),
        ),
        ("techs_unlocked", Box::new(|s| f64::from(s.techs_unlocked))),
        (
            "avg_module_wear",
            Box::new(|s| f64::from(s.avg_module_wear)),
        ),
        (
            "repair_kits_remaining",
            Box::new(|s| f64::from(s.repair_kits_remaining)),
        ),
    ];

    let metrics = extractors
        .iter()
        .map(|(name, extract)| {
            let values: Vec<f64> = snapshots.iter().map(|(_, s)| extract(s)).collect();
            compute_metric_summary(name, &values)
        })
        .collect();

    SummaryStats {
        seed_count,
        collapsed_count,
        metrics,
    }
}

fn compute_metric_summary(name: &str, values: &[f64]) -> MetricSummary {
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    let stddev = variance.sqrt();

    MetricSummary {
        name: name.to_string(),
        mean,
        min,
        max,
        stddev,
    }
}

pub fn print_summary(scenario_name: &str, ticks: u64, stats: &SummaryStats) {
    let tick_display = if ticks >= 1000 {
        format!("{}k", ticks / 1000)
    } else {
        ticks.to_string()
    };
    println!(
        "\n=== {} ({} seeds, {} ticks each) ===\n",
        scenario_name, stats.seed_count, tick_display
    );
    println!(
        "{:<30} {:>8} {:>8} {:>8} {:>8}",
        "Metric", "Mean", "Min", "Max", "StdDev"
    );
    println!("{}", "-".repeat(70));
    for metric in &stats.metrics {
        println!(
            "{:<30} {:>8.2} {:>8.2} {:>8.2} {:>8.2}",
            metric.name, metric.mean, metric.min, metric.max, metric.stddev
        );
    }
    println!(
        "{:<30} {}/{}",
        "collapse_rate", stats.collapsed_count, stats.seed_count
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(
        tick: u64,
        storage_pct: f32,
        fleet_total: u32,
        fleet_idle: u32,
        refinery_starved: u32,
        techs: u32,
        avg_wear: f32,
        repair_kits: u32,
    ) -> MetricsSnapshot {
        MetricsSnapshot {
            tick,
            metrics_version: 3,
            total_ore_kg: 0.0,
            total_material_kg: 0.0,
            total_slag_kg: 0.0,
            total_iron_material_kg: 0.0,
            station_storage_used_pct: storage_pct,
            ship_cargo_used_pct: 0.0,
            avg_ore_fe_fraction: 0.0,
            ore_lot_count: 0,
            min_ore_fe_fraction: 0.0,
            max_ore_fe_fraction: 0.0,
            avg_material_quality: 0.0,
            refinery_active_count: 0,
            refinery_starved_count: refinery_starved,
            refinery_stalled_count: 0,
            assembler_active_count: 0,
            assembler_stalled_count: 0,
            fleet_total,
            fleet_idle,
            fleet_mining: 0,
            fleet_transiting: 0,
            fleet_surveying: 0,
            fleet_depositing: 0,
            scan_sites_remaining: 0,
            asteroids_discovered: 0,
            asteroids_depleted: 0,
            techs_unlocked: techs,
            total_scan_data: 0.0,
            max_tech_evidence: 0.0,
            avg_module_wear: avg_wear,
            max_module_wear: 0.0,
            repair_kits_remaining: repair_kits,
        }
    }

    #[test]
    fn test_summary_basic_stats() {
        let s1 = make_snapshot(100, 0.5, 2, 0, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.7, 2, 0, 0, 5, 0.4, 3);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &s1), (2, &s2)];
        let stats = compute_summary(&snapshots);

        assert_eq!(stats.seed_count, 2);
        assert_eq!(stats.collapsed_count, 0);

        let storage = &stats.metrics[0];
        assert_eq!(storage.name, "storage_saturation_pct");
        assert!((storage.mean - 0.6).abs() < 1e-5);
        assert!((storage.min - 0.5).abs() < 1e-5);
        assert!((storage.max - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_collapse_detection() {
        // Collapsed: refinery_starved > 0 AND fleet_idle == fleet_total
        let collapsed = make_snapshot(100, 0.5, 2, 2, 1, 3, 0.2, 5);
        let healthy = make_snapshot(100, 0.5, 2, 0, 0, 3, 0.2, 5);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &collapsed), (2, &healthy)];
        let stats = compute_summary(&snapshots);

        assert_eq!(stats.collapsed_count, 1);
    }

    #[test]
    fn test_stddev_zero_for_identical() {
        let s1 = make_snapshot(100, 0.5, 2, 1, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.5, 2, 1, 0, 3, 0.2, 5);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &s1), (2, &s2)];
        let stats = compute_summary(&snapshots);

        for metric in &stats.metrics {
            assert!(
                metric.stddev.abs() < 1e-10,
                "stddev for {} should be 0, got {}",
                metric.name,
                metric.stddev
            );
        }
    }
}
