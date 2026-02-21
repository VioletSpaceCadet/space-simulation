//! `sim_core` — deterministic simulation tick.
//!
//! No IO, no network. All randomness via the passed-in Rng.

mod composition;
mod engine;
mod graph;
mod id;
pub mod metrics;
mod research;
mod station;
pub(crate) mod tasks;
mod types;
pub mod wear;

pub use engine::tick;
pub use graph::shortest_hop_count;
pub use id::generate_uuid;
pub use metrics::{
    append_metrics_row, compute_metrics, write_metrics_csv, write_metrics_header,
    MetricsFileWriter, MetricsSnapshot,
};
pub use tasks::{inventory_volume_m3, mine_duration};
pub use types::*;
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
