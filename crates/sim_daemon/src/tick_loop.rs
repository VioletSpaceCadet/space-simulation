use crate::state::{EventTx, SharedSim, SimState};
use sim_control::CommandSource;
use sim_core::EventLevel;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub async fn run_tick_loop(
    sim: SharedSim,
    event_tx: EventTx,
    ticks_per_sec: Arc<AtomicU64>,
    max_ticks: Option<u64>,
    paused: Arc<AtomicBool>,
) {
    let mut next_tick_at: Option<std::time::Instant> = None;

    loop {
        while paused.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(50)).await;
            next_tick_at = None; // Reset pacing after unpause
        }

        // Pace: sleep only if there's time remaining before the next tick is due.
        let rate = f64::from_bits(ticks_per_sec.load(Ordering::Relaxed));
        if rate > 0.0 {
            let now = std::time::Instant::now();
            let target = next_tick_at.unwrap_or(now);
            if now < target {
                tokio::time::sleep(target - now).await;
            }
            next_tick_at = Some(
                next_tick_at
                    .unwrap_or(now)
                    .checked_add(Duration::from_secs_f64(1.0 / rate))
                    .unwrap_or(now),
            );
        } else {
            // Unlimited: yield once per tick to let the tokio runtime service SSE etc.
            tokio::task::yield_now().await;
            next_tick_at = None;
        }

        let (events, done) = {
            let mut guard = sim.lock();
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

                let state = &mut *guard;
                let history_clone = state.metrics_history.clone();
                if let Some(engine) = state.alert_engine.as_mut() {
                    let tick = state.game_state.meta.tick;
                    let alert_events =
                        engine.evaluate(&history_clone, tick, &mut state.game_state.counters);
                    events.extend(alert_events);
                }
            }

            let done = max_ticks.is_some_and(|max| guard.game_state.meta.tick >= max);
            (events, done)
        };

        let _ = event_tx.send(events);

        if done {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SimState;
    use parking_lot::Mutex;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use sim_core::test_fixtures::base_content;
    use sim_core::EventEnvelope;
    use sim_world::build_initial_state;
    use std::collections::VecDeque;
    use tokio::sync::broadcast;

    fn make_test_sim() -> (SharedSim, EventTx, Arc<AtomicBool>) {
        let content = base_content();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let game_state = build_initial_state(&content, 0, &mut rng);
        let (event_tx, _) = broadcast::channel::<Vec<EventEnvelope>>(256);
        let sim = Arc::new(Mutex::new(SimState {
            game_state,
            content,
            rng,
            autopilot: sim_control::AutopilotController,
            next_command_id: 0,
            metrics_every: 0,
            metrics_history: VecDeque::new(),
            metrics_writer: None,
            alert_engine: None,
        }));
        let paused = Arc::new(AtomicBool::new(false));
        (sim, event_tx, paused)
    }

    #[tokio::test]
    async fn test_tick_loop_advances_tick() {
        let (sim, event_tx, paused) = make_test_sim();
        run_tick_loop(
            sim.clone(),
            event_tx,
            Arc::new(AtomicU64::new(0.0_f64.to_bits())),
            Some(5),
            paused,
        )
        .await;
        let guard = sim.lock();
        assert_eq!(guard.game_state.meta.tick, 5);
    }

    #[tokio::test]
    async fn test_tick_loop_broadcasts_events() {
        let (sim, event_tx, paused) = make_test_sim();
        let mut rx = event_tx.subscribe();
        run_tick_loop(
            sim,
            event_tx,
            Arc::new(AtomicU64::new(0.0_f64.to_bits())),
            Some(3),
            paused,
        )
        .await;

        let mut received = 0;
        while rx.try_recv().is_ok() {
            received += 1;
        }
        assert!(
            received >= 3,
            "expected at least 3 event batches, got {received}"
        );
    }

    #[tokio::test]
    async fn test_tick_loop_respects_pause() {
        let (sim, event_tx, paused) = make_test_sim();
        paused.store(true, Ordering::Relaxed);

        let sim_clone = sim.clone();
        let paused_clone = paused.clone();
        let handle = tokio::spawn(async move {
            run_tick_loop(
                sim_clone,
                event_tx,
                Arc::new(AtomicU64::new(0.0_f64.to_bits())),
                Some(5),
                paused_clone,
            )
            .await;
        });

        // Give the loop time to notice it's paused (it sleeps 50ms per check).
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            sim.lock().game_state.meta.tick,
            0,
            "tick should not advance while paused"
        );

        // Unpause and let it finish.
        paused.store(false, Ordering::Relaxed);
        handle.await.unwrap();
        assert_eq!(sim.lock().game_state.meta.tick, 5);
    }

    #[tokio::test]
    async fn test_tick_loop_collects_metrics() {
        let (sim, event_tx, paused) = make_test_sim();
        sim.lock().metrics_every = 1;

        run_tick_loop(
            sim.clone(),
            event_tx,
            Arc::new(AtomicU64::new(0.0_f64.to_bits())),
            Some(5),
            paused,
        )
        .await;
        let guard = sim.lock();
        assert_eq!(
            guard.metrics_history.len(),
            5,
            "expected 5 metrics snapshots (one per tick with metrics_every=1)"
        );
    }
}
