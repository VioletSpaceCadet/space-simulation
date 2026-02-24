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
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[cfg(test)]
pub fn make_router(state: AppState) -> Router {
    make_router_with_cors(state, "http://localhost:5173")
}

pub fn make_router_with_cors(state: AppState, cors_origin: &str) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(cors_origin.parse::<axum::http::HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    Router::new()
        .route("/api/v1/meta", get(meta_handler))
        .route("/api/v1/snapshot", get(snapshot_handler))
        .route("/api/v1/metrics", get(metrics_handler))
        .route("/api/v1/stream", get(stream_handler))
        .route("/api/v1/save", post(save_handler))
        .route("/api/v1/pause", post(pause_handler))
        .route("/api/v1/resume", post(resume_handler))
        .route("/api/v1/alerts", get(alerts_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn meta_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    let sim = app_state.sim.lock();
    let ticks_per_sec = app_state.ticks_per_sec;
    let paused = app_state.paused.load(Ordering::Relaxed);
    Json(serde_json::json!({
        "tick": sim.game_state.meta.tick,
        "seed": sim.game_state.meta.seed,
        "content_version": sim.game_state.meta.content_version,
        "ticks_per_sec": ticks_per_sec,
        "paused": paused,
    }))
}

pub async fn snapshot_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let sim = app_state.sim.lock();
    match serde_json::to_string(&sim.game_state) {
        Ok(json) => {
            drop(sim);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                json,
            )
        }
        Err(err) => {
            tracing::error!("snapshot serialization failed: {err}");
            drop(sim);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            )
        }
    }
}

pub async fn metrics_handler(
    State(app_state): State<AppState>,
) -> Json<VecDeque<sim_core::MetricsSnapshot>> {
    let sim = app_state.sim.lock();
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

    let sim = app_state.sim.lock();
    let tick = sim.game_state.meta.tick;
    let body = match serde_json::to_string_pretty(&sim.game_state) {
        Ok(json) => json,
        Err(err) => {
            tracing::error!("save serialization failed: {err}");
            drop(sim);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "serialization failed"})),
            );
        }
    };
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

async fn alerts_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    let sim = app_state.sim.lock();
    let active_ids: Vec<String> = sim
        .alert_engine
        .as_ref()
        .map(|e| e.active_alert_ids())
        .unwrap_or_default();
    Json(serde_json::json!({ "active_alerts": active_ids }))
}

pub async fn pause_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    app_state.paused.store(true, Ordering::Relaxed);
    Json(serde_json::json!({"paused": true}))
}

pub async fn resume_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    app_state.paused.store(false, Ordering::Relaxed);
    Json(serde_json::json!({"paused": false}))
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
                    let tick = sim.lock().game_state.meta.tick;
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
