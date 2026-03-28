# Sim Speed Control — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add runtime sim speed control with 5 preset speeds (100, 1K, 10K, 100K, unlimited TPS), controllable via UI buttons and keyboard shortcuts 1-5.

**Architecture:** Share tick rate as an `Arc<AtomicU64>` (bits of f64) between the tick loop and route handlers. Replace fixed `tokio::time::interval` with per-tick `tokio::time::sleep`. New `POST /api/v1/speed` endpoint. UI adds speed buttons to StatusBar with keyboard shortcuts.

**Tech Stack:** Rust (axum, tokio, AtomicU64), React 19, TypeScript, Tailwind v4

---

### Task 1: Make tick rate dynamically adjustable in the daemon

**Files:**
- Modify: `crates/sim_daemon/src/state.rs:44-50` (AppState)
- Modify: `crates/sim_daemon/src/tick_loop.rs:8-82` (run_tick_loop)
- Modify: `crates/sim_daemon/src/main.rs:117-148` (AppState construction + tick loop spawn)

**Step 1: Change `ticks_per_sec` from `f64` to `Arc<AtomicU64>` in AppState**

In `state.rs`, change:

```rust
pub ticks_per_sec: f64,
```

to:

```rust
pub ticks_per_sec: Arc<std::sync::atomic::AtomicU64>,
```

Add `use std::sync::atomic::AtomicU64;` at the top. The AtomicU64 stores the bits of the f64 via `f64::to_bits()` / `f64::from_bits()`.

**Step 2: Update tick_loop to read dynamic rate each iteration**

Replace the entire `run_tick_loop` function. The new version:
- Takes `ticks_per_sec: Arc<AtomicU64>` instead of `f64`
- Removes the fixed `interval` created once at startup
- Each iteration: reads the atomic, computes sleep duration, uses `tokio::time::sleep`

```rust
pub async fn run_tick_loop(
    sim: SharedSim,
    event_tx: EventTx,
    ticks_per_sec: Arc<std::sync::atomic::AtomicU64>,
    max_ticks: Option<u64>,
    paused: Arc<AtomicBool>,
) {
    loop {
        while paused.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // ... (existing tick logic unchanged — lines 28-69) ...

        let _ = event_tx.send(events);

        if done {
            break;
        }

        let rate = f64::from_bits(ticks_per_sec.load(Ordering::Relaxed));
        if rate > 0.0 {
            tokio::time::sleep(Duration::from_secs_f64(1.0 / rate)).await;
        } else {
            tokio::task::yield_now().await;
        }
    }
}
```

**Step 3: Update main.rs to construct `Arc<AtomicU64>` and pass it**

In `main.rs`, where `AppState` is constructed (~line 117):

```rust
let ticks_per_sec_atomic = Arc::new(AtomicU64::new(ticks_per_sec.to_bits()));
```

Pass it to both `AppState` and `run_tick_loop`.

**Step 4: Update meta_handler to read from the atomic**

In `routes.rs`, `meta_handler` currently reads `app_state.ticks_per_sec` as f64. Change to:

```rust
let ticks_per_sec = f64::from_bits(app_state.ticks_per_sec.load(Ordering::Relaxed));
```

**Step 5: Fix all compilation errors**

The test helpers in `main.rs` and `tick_loop.rs` construct `AppState` and call `run_tick_loop` — update them to use `Arc<AtomicU64>`. For test helpers:

```rust
ticks_per_sec: Arc::new(AtomicU64::new(10.0_f64.to_bits())),
```

For `run_tick_loop` calls in tests:

```rust
run_tick_loop(sim.clone(), event_tx, Arc::new(AtomicU64::new(0.0_f64.to_bits())), Some(5), paused).await;
```

**Step 6: Run tests**

Run: `cargo test -p sim_daemon`
Expected: all existing tests pass

**Step 7: Commit**

```
feat(daemon): make tick rate dynamically adjustable via AtomicU64
```

---

### Task 2: Add `POST /api/v1/speed` endpoint

**Files:**
- Modify: `crates/sim_daemon/src/routes.rs:26-44` (router), add speed_handler
- Modify: `crates/sim_daemon/src/main.rs:156-576` (tests)

**Step 1: Write the test**

In `main.rs` tests, add:

```rust
#[tokio::test]
async fn test_speed_sets_ticks_per_sec() {
    let state = make_test_state();
    let app = make_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/speed")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ticks_per_sec": 1000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ticks_per_sec"], 1000.0);

    // Verify meta reflects new speed
    let rate = f64::from_bits(state.ticks_per_sec.load(std::sync::atomic::Ordering::Relaxed));
    assert!((rate - 1000.0).abs() < 0.001);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_daemon test_speed_sets_ticks_per_sec`
Expected: FAIL (no route)

**Step 3: Add the route and handler**

In `routes.rs`, add the route:

```rust
.route("/api/v1/speed", post(speed_handler))
```

Add the handler:

```rust
pub async fn speed_handler(
    State(app_state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(tps) = body.get("ticks_per_sec").and_then(|v| v.as_f64()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "missing or invalid ticks_per_sec"})),
        );
    };
    if tps < 0.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "ticks_per_sec must be >= 0"})),
        );
    }
    app_state.ticks_per_sec.store(tps.to_bits(), Ordering::Relaxed);
    (
        StatusCode::OK,
        Json(serde_json::json!({"ticks_per_sec": tps})),
    )
}
```

**Step 4: Run tests**

Run: `cargo test -p sim_daemon`
Expected: all pass

**Step 5: Commit**

```
feat(daemon): add POST /api/v1/speed endpoint for runtime tick rate control
```

---

### Task 3: Add `setSpeed` API function in the UI

**Files:**
- Modify: `ui_web/src/api.ts` — add `setSpeed()`
- Modify: `ui_web/src/api.test.ts` — add test

**Step 1: Add the API function**

In `api.ts`, add:

```typescript
export async function setSpeed(ticksPerSec: number): Promise<{ ticks_per_sec: number }> {
  const response = await fetch('/api/v1/speed', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ticks_per_sec: ticksPerSec }),
  })
  if (!response.ok) throw new Error(`Speed change failed: ${response.status}`)
  return response.json()
}
```

**Step 2: Add test**

In `api.test.ts`, add:

```typescript
describe('setSpeed', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('sends POST to /api/v1/speed with ticks_per_sec', async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ ticks_per_sec: 1000 }))
    )
    const { setSpeed } = await import('./api')
    await setSpeed(1000)
    expect(global.fetch).toHaveBeenCalledWith('/api/v1/speed', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ticks_per_sec: 1000 }),
    })
  })
})
```

Update the import at the top of the test file to include `setSpeed`.

**Step 3: Run tests**

Run: `cd ui_web && npm test`
Expected: all pass

**Step 4: Commit**

```
feat(ui): add setSpeed API function
```

---

### Task 4: Add speed buttons and keyboard shortcuts to StatusBar

**Files:**
- Modify: `ui_web/src/components/StatusBar.tsx` — add speed buttons
- Modify: `ui_web/src/App.tsx` — add speed state, keyboard shortcuts, pass props

**Step 1: Update StatusBar props and add speed buttons**

Add to `StatusBar` Props interface:

```typescript
activeSpeed: number  // current ticks_per_sec (0 = max)
onSetSpeed: (tps: number) => void
```

Add speed preset buttons between the alert badges and the pause button. The 5 presets:

```typescript
const SPEED_PRESETS = [
  { label: '100', tps: 100 },
  { label: '1K', tps: 1_000 },
  { label: '10K', tps: 10_000 },
  { label: '100K', tps: 100_000 },
  { label: 'Max', tps: 0 },
] as const
```

Render them as a button group:

```tsx
<div className="flex items-center gap-0.5">
  {SPEED_PRESETS.map(({ label, tps }) => (
    <button
      key={label}
      type="button"
      onClick={() => onSetSpeed(tps)}
      className={`px-2 py-0.5 rounded-sm text-[10px] uppercase tracking-widest transition-colors cursor-pointer border ${
        activeSpeed === tps
          ? 'border-accent/40 text-accent'
          : 'border-edge text-muted hover:text-dim hover:border-dim'
      }`}
    >
      {label}
    </button>
  ))}
</div>
```

Place this group in the `ml-auto flex items-center gap-3` div, before the pause button.

**Step 2: Update App.tsx — speed state and handler**

Add import:

```typescript
import { fetchMeta, pauseGame, resumeGame, setSpeed } from './api'
```

Add speed handler (near `handleTogglePause`):

```typescript
const handleSetSpeed = useCallback((tps: number) => {
  setTicksPerSec(tps)
  setSpeed(tps).catch(() => {
    // Revert on failure — re-fetch meta to get actual speed
    fetchMeta().then((meta) => setTicksPerSec(meta.ticks_per_sec)).catch(() => {})
  })
}, [])
```

Pass new props to StatusBar:

```tsx
<StatusBar
  tick={displayTick}
  connected={connected}
  measuredTickRate={measuredTickRate}
  paused={paused}
  onTogglePause={handleTogglePause}
  alerts={activeAlerts}
  dismissedAlerts={dismissedAlerts}
  onDismissAlert={dismissAlert}
  activeSpeed={ticksPerSec}
  onSetSpeed={handleSetSpeed}
/>
```

**Step 3: Add keyboard shortcuts in App.tsx**

In the existing `handleKeyDown` effect (lines 55-66), add number key handling:

```typescript
const SPEED_KEYS: Record<string, number> = {
  'Digit1': 100,
  'Digit2': 1_000,
  'Digit3': 10_000,
  'Digit4': 100_000,
  'Digit5': 0,
  'Numpad1': 100,
  'Numpad2': 1_000,
  'Numpad3': 10_000,
  'Numpad4': 100_000,
  'Numpad5': 0,
}
```

In the keydown handler, after the Space check:

```typescript
const speedTps = SPEED_KEYS[event.code]
if (speedTps !== undefined) {
  handleSetSpeed(speedTps)
  return
}
```

Note: `SPEED_KEYS` lookup uses `event.code` (not `event.key`) so numpad works correctly. The `handleSetSpeed` must be in the effect's dependency array.

**Step 4: Run UI tests and verify manually**

Run: `cd ui_web && npm test`

**Step 5: Commit**

```
feat(ui): add speed control buttons and 1-5 keyboard shortcuts
```

---

### Task 5: Update CLAUDE.md and reference docs

**Files:**
- Modify: `CLAUDE.md` — update daemon endpoints list, keyboard shortcuts
- Modify: `docs/reference.md` — update daemon API docs if present

**Step 1: Update CLAUDE.md**

In the sim_daemon section, update endpoints list to include `/api/v1/speed`. In ui_web section, add speed buttons and 1-5 keyboard shortcuts.

**Step 2: Commit**

```
docs: update CLAUDE.md with speed control endpoint and shortcuts
```
