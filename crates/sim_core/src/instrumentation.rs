use std::time::Duration;

/// Per-step timing data for a single tick.
///
/// 17 duration fields: 9 top-level tick steps + 8 station sub-steps.
/// Station sub-steps are aggregated across all stations (not per-station).
///
/// Active in debug builds by default; compiled away in release builds unless
/// the `instrumentation` feature is enabled.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TickTimings {
    // Top-level steps (8)
    pub apply_commands: Duration,
    pub resolve_ship_tasks: Duration,
    pub tick_stations: Duration,
    pub tick_ground_facilities: Duration,
    pub tick_satellites: Duration,
    pub advance_research: Duration,
    pub evaluate_milestones: Duration,
    pub evaluate_events: Duration,
    pub replenish_scan_sites: Duration,

    // Station sub-steps (8), aggregated across all stations
    pub power_budget: Duration,
    pub processors: Duration,
    pub assemblers: Duration,
    pub sensors: Duration,
    pub labs: Duration,
    pub maintenance: Duration,
    pub thermal: Duration,
    pub boiloff: Duration,
}

impl TickTimings {
    /// Returns an iterator over all (name, duration) pairs.
    pub fn iter_fields(&self) -> impl Iterator<Item = (&'static str, Duration)> + '_ {
        [
            ("apply_commands", self.apply_commands),
            ("resolve_ship_tasks", self.resolve_ship_tasks),
            ("tick_stations", self.tick_stations),
            ("tick_ground_facilities", self.tick_ground_facilities),
            ("tick_satellites", self.tick_satellites),
            ("advance_research", self.advance_research),
            ("evaluate_milestones", self.evaluate_milestones),
            ("evaluate_events", self.evaluate_events),
            ("replenish_scan_sites", self.replenish_scan_sites),
            ("power_budget", self.power_budget),
            ("processors", self.processors),
            ("assemblers", self.assemblers),
            ("sensors", self.sensors),
            ("labs", self.labs),
            ("maintenance", self.maintenance),
            ("thermal", self.thermal),
            ("boiloff", self.boiloff),
        ]
        .into_iter()
    }
}

/// Summary statistics for a single tick step.
#[derive(Debug, Clone)]
pub struct StepStats {
    pub name: String,
    pub mean_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
}

/// Compute per-step summary statistics from a collection of `TickTimings`.
///
/// Returns one `StepStats` entry per field (16 total), with mean/p50/p95/max
/// in microseconds.
pub fn compute_step_stats(timings: &[TickTimings]) -> Vec<StepStats> {
    if timings.is_empty() {
        return Vec::new();
    }

    let sample = &timings[0];
    let field_names: Vec<&str> = sample.iter_fields().map(|(name, _)| name).collect();

    field_names
        .iter()
        .enumerate()
        .map(|(field_index, &name)| {
            let mut values_us: Vec<f64> = timings
                .iter()
                .map(|t| {
                    t.iter_fields()
                        .nth(field_index)
                        .map_or(0.0, |(_, d)| d.as_secs_f64() * 1_000_000.0)
                })
                .collect();
            values_us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let count = values_us.len();
            let mean_us = values_us.iter().sum::<f64>() / count as f64;

            StepStats {
                name: name.to_string(),
                mean_us,
                p50_us: percentile_of(&values_us, 50.0),
                p95_us: percentile_of(&values_us, 95.0),
                max_us: values_us.last().copied().unwrap_or(0.0),
            }
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn percentile_of(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = (pct / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// Wraps a statement with timing instrumentation.
///
/// Active in debug builds and when the `instrumentation` feature is enabled;
/// compiled away in release builds without the feature.
/// Zero-cost when `timings` is `None` — no `Instant::now()` calls.
///
/// Note: the expression's return value is discarded (the macro evaluates to `()`).
/// Only use with expressions that return `()`.
///
/// Usage: `timed!(timings_option, field_name, expression);`
macro_rules! timed {
    ($timings:expr, $field:ident, $body:expr) => {{
        #[cfg(any(debug_assertions, feature = "instrumentation"))]
        {
            if $timings.is_some() {
                let __start = std::time::Instant::now();
                $body;
                if let Some(ref mut __t) = $timings {
                    __t.$field += __start.elapsed();
                }
            } else {
                $body;
            }
        }
        #[cfg(not(any(debug_assertions, feature = "instrumentation")))]
        {
            $body;
        }
    }};
}
pub(crate) use timed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_timings_default_is_zero() {
        let timings = TickTimings::default();
        for (name, duration) in timings.iter_fields() {
            assert_eq!(duration, Duration::ZERO, "{name} should be zero by default");
        }
    }

    #[test]
    fn tick_timings_field_count() {
        let timings = TickTimings::default();
        assert_eq!(timings.iter_fields().count(), 17);
    }

    #[test]
    fn timed_macro_records_when_some() {
        let mut timings = TickTimings::default();
        let mut opt = Some(&mut timings);
        timed!(
            opt,
            apply_commands,
            std::thread::sleep(Duration::from_micros(10))
        );
        assert!(
            timings.apply_commands > Duration::ZERO,
            "should have recorded time"
        );
    }

    #[test]
    fn timed_macro_noop_when_none() {
        let mut opt: Option<&mut TickTimings> = None;
        let mut counter = 0;
        timed!(opt, apply_commands, counter += 1);
        assert_eq!(counter, 1, "body should still execute when None");
    }

    #[test]
    fn timed_macro_accumulates_across_calls() {
        let mut timings = TickTimings::default();
        let mut opt = Some(&mut timings);
        timed!(
            opt,
            processors,
            std::thread::sleep(Duration::from_micros(10))
        );
        timed!(
            opt,
            processors,
            std::thread::sleep(Duration::from_micros(10))
        );
        assert!(
            timings.processors >= Duration::from_micros(20),
            "should accumulate: {:?}",
            timings.processors
        );
    }

    #[test]
    fn compute_step_stats_empty_input() {
        let stats = compute_step_stats(&[]);
        assert!(stats.is_empty());
    }

    #[test]
    fn compute_step_stats_entry_count() {
        let timings = vec![TickTimings::default(); 10];
        let stats = compute_step_stats(&timings);
        assert_eq!(stats.len(), 17);
        assert_eq!(stats[0].name, "apply_commands");
        assert_eq!(stats[16].name, "boiloff");
    }

    #[test]
    fn compute_step_stats_nonzero_values() {
        let mut t1 = TickTimings::default();
        t1.apply_commands = Duration::from_micros(100);
        let mut t2 = TickTimings::default();
        t2.apply_commands = Duration::from_micros(200);

        let stats = compute_step_stats(&[t1, t2]);
        let apply = &stats[0];
        assert!((apply.mean_us - 150.0).abs() < 1.0, "mean should be ~150");
        assert!((apply.max_us - 200.0).abs() < 1.0, "max should be ~200");
    }
}
