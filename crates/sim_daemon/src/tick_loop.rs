use crate::state::{EventTx, SharedSim, SimState};
use sim_control::CommandSource;
use sim_core::EventLevel;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub async fn run_tick_loop(
    sim: SharedSim,
    event_tx: EventTx,
    ticks_per_sec: f64,
    max_ticks: Option<u64>,
    paused: Arc<AtomicBool>,
) {
    let mut interval = if ticks_per_sec > 0.0 {
        let mut iv = tokio::time::interval(Duration::from_secs_f64(1.0 / ticks_per_sec));
        iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
        Some(iv)
    } else {
        None
    };

    loop {
        while paused.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let (events, done) = {
            let mut guard = sim.lock().unwrap();
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
            let mut events =
                sim_core::tick(game_state, &commands, content, rng, EventLevel::Normal);

            let metrics_every = guard.metrics_every;
            if metrics_every > 0 && guard.game_state.meta.tick.is_multiple_of(metrics_every) {
                let snapshot = sim_core::compute_metrics(&guard.game_state, &guard.content);
                guard.push_metrics(snapshot);

                // Evaluate alert rules against the in-memory metrics history.
                // Clone history to avoid simultaneous immutable + mutable borrows on guard.
                if let Some(mut engine) = guard.alert_engine.take() {
                    let tick = guard.game_state.meta.tick;
                    let history_clone = guard.metrics_history.clone();
                    let alert_events =
                        engine.evaluate(&history_clone, tick, &mut guard.game_state.counters);
                    events.extend(alert_events);
                    guard.alert_engine = Some(engine);
                }
            }

            let done = max_ticks.is_some_and(|max| guard.game_state.meta.tick >= max);
            (events, done)
        };

        let _ = event_tx.send(events);

        if done {
            break;
        }

        if let Some(ref mut iv) = interval {
            iv.tick().await;
        } else {
            tokio::task::yield_now().await;
        }
    }
}
