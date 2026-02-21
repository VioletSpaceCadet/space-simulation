use rand_chacha::ChaCha8Rng;
use sim_control::AutopilotController;
use sim_core::{EventEnvelope, GameContent, GameState, MetricsFileWriter, MetricsSnapshot};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// Maximum number of metrics snapshots kept in memory.
const MAX_METRICS_HISTORY: usize = 10_000;

pub struct SimState {
    pub game_state: GameState,
    pub content: GameContent,
    pub rng: ChaCha8Rng,
    pub autopilot: AutopilotController,
    pub next_command_id: u64,
    pub metrics_every: u64,
    pub metrics_history: VecDeque<MetricsSnapshot>,
    pub metrics_writer: Option<MetricsFileWriter>,
}

impl SimState {
    pub fn push_metrics(&mut self, snapshot: MetricsSnapshot) {
        if self.metrics_history.len() >= MAX_METRICS_HISTORY {
            self.metrics_history.pop_front();
        }
        if let Some(ref mut writer) = self.metrics_writer {
            let _ = writer.write_row(&snapshot);
        }
        self.metrics_history.push_back(snapshot);
    }
}

pub type SharedSim = Arc<Mutex<SimState>>;
pub type EventTx = broadcast::Sender<Vec<EventEnvelope>>;

#[derive(Clone)]
pub struct AppState {
    pub sim: SharedSim,
    pub event_tx: EventTx,
    pub ticks_per_sec: f64,
}
