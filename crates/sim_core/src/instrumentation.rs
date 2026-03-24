use std::time::Duration;

/// Per-step timing data for a single tick.
///
/// 14 duration fields: 6 top-level tick steps + 8 station sub-steps.
/// Station sub-steps are aggregated across all stations (not per-station).
///
/// Active in debug builds by default; compiled away in release builds unless
/// the `instrumentation` feature is enabled.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TickTimings {
    // Top-level steps (6)
    pub apply_commands: Duration,
    pub resolve_ship_tasks: Duration,
    pub tick_stations: Duration,
    pub advance_research: Duration,
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
            ("advance_research", self.advance_research),
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

/// Wraps a statement with timing instrumentation.
///
/// Active in debug builds and when the `instrumentation` feature is enabled;
/// compiled away in release builds without the feature.
/// Zero-cost when `timings` is `None` — no `Instant::now()` calls.
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
    fn tick_timings_has_14_fields() {
        let timings = TickTimings::default();
        assert_eq!(timings.iter_fields().count(), 14);
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
}
