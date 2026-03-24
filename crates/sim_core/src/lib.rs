//! `sim_core` — deterministic simulation tick.
//!
//! No IO, no network. All randomness via the passed-in Rng.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub(crate) mod commands;
mod composition;
mod engine;
mod id;
pub mod instrumentation;
pub mod metrics;
pub mod modifiers;
mod research;
pub mod sim_events;
pub mod spatial;
mod station;
pub(crate) mod tasks;
pub mod thermal;
pub mod trade;
mod types;
pub mod wear;

pub use commands::recompute_ship_stats;
pub use engine::{tick, trade_unlock_tick};
pub use id::generate_uuid;
pub use instrumentation::{compute_step_stats, StepStats, TickTimings};
pub use metrics::{
    append_metrics_row, compute_metrics, content_behavior_types, content_element_ids,
    write_metrics_csv, write_metrics_header, MetricType, MetricValue, MetricsFileWriter,
    MetricsSnapshot, ModuleStatusMetrics, OreElementStats, METRICS_VERSION,
};
pub use spatial::{
    build_body_cache, compute_entity_absolute, integer_sqrt, is_co_located, pick_template_biased,
    pick_zone_weighted, polar_to_cart, random_angle_in_span, random_position_in_zone,
    random_radius_in_band, travel_ticks, AbsolutePos, AngleMilliDeg, BodyCache, EntityCache,
    Position, RadiusAuMicro, ResourceClass, FULL_CIRCLE, METERS_PER_AU, METERS_PER_MICRO_AU,
};
pub use tasks::{inventory_volume_m3, mine_duration};
pub use types::*;
// Explicit re-export — glob re-export of type aliases can be unreliable across Rust versions
pub use types::AHashMap;
pub use wear::wear_efficiency;

pub(crate) fn emit(counters: &mut Counters, tick: u64, event: Event) -> EventEnvelope {
    let id = EventId(format!("evt_{:06}", counters.next_event_id));
    counters.next_event_id += 1;
    EventEnvelope { id, tick, event }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_fixtures;
#[cfg(test)]
mod tests;
