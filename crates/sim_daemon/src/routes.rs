use crate::state::AppState;
use axum::{
    extract::State,
    http::{header, Method, StatusCode},
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::{get, post},
    Router,
};
use sim_core::EventEnvelope;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

pub fn make_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            "http://localhost:5173"
                .parse::<axum::http::HeaderValue>()
                .unwrap(),
        )
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    Router::new()
        .route("/api/v1/meta", get(meta_handler))
        .route("/api/v1/snapshot", get(snapshot_handler))
        .route("/api/v1/metrics", get(metrics_handler))
        .route("/api/v1/stream", get(stream_handler))
        .route("/api/v1/save", post(save_handler))
        .layer(cors)
        .with_state(state)
}

pub async fn meta_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    let sim = app_state.sim.lock().unwrap();
    let ticks_per_sec = app_state.ticks_per_sec;
    Json(serde_json::json!({
        "tick": sim.game_state.meta.tick,
        "seed": sim.game_state.meta.seed,
        "content_version": sim.game_state.meta.content_version,
        "ticks_per_sec": ticks_per_sec,
    }))
}

pub async fn snapshot_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let sim = app_state.sim.lock().unwrap();
    let body = serde_json::to_string(&sim.game_state).unwrap_or_default();
    drop(sim);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
}

pub async fn metrics_handler(
    State(app_state): State<AppState>,
) -> Json<VecDeque<sim_core::MetricsSnapshot>> {
    let sim = app_state.sim.lock().unwrap();
    Json(sim.metrics_history.clone())
}

pub async fn save_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let run_dir = match &app_state.run_dir {
        Some(dir) => dir.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "no run directory (started with --no-metrics?)"})),
            );
        }
    };

    let sim = app_state.sim.lock().unwrap();
    let tick = sim.game_state.meta.tick;
    let body = serde_json::to_string_pretty(&sim.game_state).unwrap_or_default();
    drop(sim);

    let saves_dir = run_dir.join("saves");
    if let Err(err) = std::fs::create_dir_all(&saves_dir) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("create saves dir: {err}")})),
        );
    }

    let filename = format!("save_{tick}.json");
    let path = saves_dir.join(&filename);
    if let Err(err) = std::fs::write(&path, body) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("write save: {err}")})),
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"path": path.display().to_string(), "tick": tick})),
    )
}

pub async fn stream_handler(
    State(app_state): State<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = app_state.event_tx.subscribe();
    let sim = app_state.sim.clone();

    let stream = async_stream::stream! {
        let mut heartbeat = tokio::time::interval(Duration::from_millis(200));
        heartbeat.tick().await; // discard the immediate first tick
        let mut flush = tokio::time::interval(Duration::from_millis(50));
        flush.tick().await; // discard the immediate first tick
        let mut pending: Vec<EventEnvelope> = Vec::new();
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(events) => pending.extend(events),
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = flush.tick() => {
                    if !pending.is_empty() {
                        let data = serde_json::to_string(&pending).unwrap_or_default();
                        pending.clear();
                        yield Ok(Event::default().data(data));
                    }
                }
                _ = heartbeat.tick() => {
                    let tick = sim.lock().unwrap().game_state.meta.tick;
                    let hb = serde_json::json!({"heartbeat": true, "tick": tick});
                    yield Ok(Event::default().data(hb.to_string()));
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    )
}
