use std::time::Duration;
use sim_control::CommandSource;
use sim_core::EventLevel;
use crate::state::{EventTx, SharedSim, SimState};

pub async fn run_tick_loop(
    sim: SharedSim,
    event_tx: EventTx,
    ticks_per_sec: f64,
    max_ticks: Option<u64>,
) {
    let sleep_duration = if ticks_per_sec > 0.0 {
        Some(Duration::from_secs_f64(1.0 / ticks_per_sec))
    } else {
        None
    };

    loop {
        let start = std::time::Instant::now();

        let (events, done) = {
            let mut guard = sim.lock().unwrap();
            // Split borrow: autopilot needs &mut self, plus we need &game_state, &content, &mut next_command_id.
            // Extract the fields we need as separate borrows to satisfy the borrow checker.
            let SimState {
                ref game_state,
                ref content,
                rng: _,
                ref mut autopilot,
                ref mut next_command_id,
                ..
            } = *guard;
            let commands = autopilot.generate_commands(game_state, content, next_command_id);
            let SimState {
                ref mut game_state,
                ref content,
                ref mut rng,
                ..
            } = *guard;
            let events = sim_core::tick(game_state, &commands, content, rng, EventLevel::Normal);
            let done = max_ticks.map_or(false, |max| guard.game_state.meta.tick >= max);
            (events, done)
        };

        let _ = event_tx.send(events);

        if done {
            break;
        }

        if let Some(duration) = sleep_duration {
            let elapsed = start.elapsed();
            if elapsed < duration {
                tokio::time::sleep(duration - elapsed).await;
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
}
