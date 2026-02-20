use rand_chacha::ChaCha8Rng;
use sim_control::AutopilotController;
use sim_core::{EventEnvelope, GameContent, GameState};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub struct SimState {
    pub game_state: GameState,
    pub content: GameContent,
    pub rng: ChaCha8Rng,
    pub autopilot: AutopilotController,
    pub next_command_id: u64,
}

pub type SharedSim = Arc<Mutex<SimState>>;
pub type EventTx = broadcast::Sender<Vec<EventEnvelope>>;

#[derive(Clone)]
pub struct AppState {
    pub sim: SharedSim,
    pub event_tx: EventTx,
}
