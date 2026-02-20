# Smooth Streaming Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the FE feel like a real-time game with 60fps ship movement, continuously updating tick counter, and task progress bars via client-side tick interpolation.

**Architecture:** New `useAnimatedTick` hook runs a `requestAnimationFrame` loop that estimates the current tick from measured server tick rate, producing a floating-point `displayTick` at 60fps. Backend sends configured `ticks_per_sec` via `/meta` and heartbeats every 200ms for frequent corrections.

**Tech Stack:** React 18, TypeScript 5, Vitest, axum 0.7 (Rust)

**Design doc:** `docs/plans/2026-02-20-smooth-streaming-design.md`

---

### Task 1: Backend — add `ticks_per_sec` to `/meta` and reduce heartbeat to 200ms

**Context:** The daemon's `/api/v1/meta` endpoint currently returns `tick`, `seed`, `content_version`. The FE needs the configured tick rate as a seed value for interpolation. The heartbeat is currently 5s — needs to be 200ms for frequent corrections.

**Files:**
- Modify: `crates/sim_daemon/src/state.rs`
- Modify: `crates/sim_daemon/src/main.rs`
- Modify: `crates/sim_daemon/src/routes.rs`
- Test: `crates/sim_daemon/src/main.rs` (existing test module)

**Step 1: Add `ticks_per_sec` to `AppState`**

In `crates/sim_daemon/src/state.rs`, add a `ticks_per_sec: f64` field to `AppState`:

```rust
#[derive(Clone)]
pub struct AppState {
    pub sim: SharedSim,
    pub event_tx: EventTx,
    pub ticks_per_sec: f64,
}
```

**Step 2: Thread `ticks_per_sec` into `AppState` in `main.rs`**

In `crates/sim_daemon/src/main.rs`, update the `AppState` construction (~line 60):

```rust
let app_state = AppState {
    sim: Arc::new(Mutex::new(SimState {
        game_state,
        content,
        rng,
        autopilot: AutopilotController,
        next_command_id: 0,
    })),
    event_tx: event_tx.clone(),
    ticks_per_sec,
};
```

Also update `make_test_state()` in the test module to include `ticks_per_sec: 10.0`.

**Step 3: Add `ticks_per_sec` to meta handler response**

In `crates/sim_daemon/src/routes.rs`, update `meta_handler` (~line 36):

```rust
pub async fn meta_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    let sim = app_state.sim.lock().unwrap();
    Json(serde_json::json!({
        "tick": sim.game_state.meta.tick,
        "seed": sim.game_state.meta.seed,
        "content_version": sim.game_state.meta.content_version,
        "ticks_per_sec": app_state.ticks_per_sec,
    }))
}
```

**Step 4: Reduce heartbeat interval from 5s to 200ms**

In `crates/sim_daemon/src/routes.rs`, update `stream_handler` (~line 65):

```rust
let mut heartbeat = tokio::time::interval(Duration::from_millis(200));
```

**Step 5: Add test for `ticks_per_sec` in meta**

Add a new test in `crates/sim_daemon/src/main.rs` test module:

```rust
#[tokio::test]
async fn test_meta_contains_ticks_per_sec() {
    let app = make_router(make_test_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/meta")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ticks_per_sec"], 10.0);
}
```

**Step 6: Run tests**

Run: `cargo test`
Expected: All tests pass including the new `test_meta_contains_ticks_per_sec`.

**Step 7: Commit**

```bash
git add crates/sim_daemon/src/state.rs crates/sim_daemon/src/main.rs crates/sim_daemon/src/routes.rs
git commit -m "feat(daemon): add ticks_per_sec to /meta, reduce heartbeat to 200ms"
```

---

### Task 2: FE — update `MetaInfo` type and fetch in App

**Context:** The FE `MetaInfo` type needs a `ticks_per_sec` field. App.tsx needs to fetch `/meta` on mount and pass the value downstream to the animated tick hook (Task 3).

**Files:**
- Modify: `ui_web/src/types.ts`
- Modify: `ui_web/src/App.tsx`
- Modify: `ui_web/src/api.test.ts`

**Step 1: Add `ticks_per_sec` to `MetaInfo` type**

In `ui_web/src/types.ts`, update the `MetaInfo` interface:

```typescript
export interface MetaInfo {
  tick: number
  seed: number
  content_version: string
  ticks_per_sec: number
}
```

**Step 2: Add test for `fetchMeta` returning `ticks_per_sec`**

In `ui_web/src/api.test.ts`, add a test in the `fetchMeta` describe block:

```typescript
it('returns parsed meta with ticks_per_sec', async () => {
  vi.mocked(global.fetch).mockResolvedValueOnce(
    new Response(JSON.stringify({ tick: 0, seed: 1, content_version: 'test', ticks_per_sec: 50 }))
  )
  const result = await fetchMeta()
  expect(result.ticks_per_sec).toBe(50)
})
```

**Step 3: Fetch meta in App.tsx and store `ticksPerSec`**

In `ui_web/src/App.tsx`, add a `useEffect` + `useState` to fetch `/meta` on mount:

```typescript
import { useCallback, useEffect, useState } from 'react'
import { fetchMeta } from './api'
// ... existing imports ...

export default function App() {
  const { snapshot, events, connected, currentTick, oreCompositions } = useSimStream()
  const { visible, toggle } = useVisiblePanels()
  const [ticksPerSec, setTicksPerSec] = useState(10) // default fallback

  useEffect(() => {
    fetchMeta().then((meta) => setTicksPerSec(meta.ticks_per_sec)).catch(() => {})
  }, [])

  // ... rest unchanged for now, displayTick wiring comes in Task 4 ...
```

**Step 4: Run tests**

Run: `cd ui_web && npm test -- --run`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add ui_web/src/types.ts ui_web/src/App.tsx ui_web/src/api.test.ts
git commit -m "feat(ui): add ticks_per_sec to MetaInfo, fetch /meta in App"
```

---

### Task 3: FE — create `useAnimatedTick` hook with tests

**Context:** This is the core new piece. The hook takes `serverTick` and `initialTickRate`, runs a `requestAnimationFrame` loop, measures actual tick rate from server samples, and outputs a continuously advancing `displayTick` at 60fps. This task creates the hook and its tests but does NOT wire it into App yet.

**Files:**
- Create: `ui_web/src/hooks/useAnimatedTick.ts`
- Create: `ui_web/src/hooks/useAnimatedTick.test.ts`

**Step 1: Write the tests**

Create `ui_web/src/hooks/useAnimatedTick.test.ts`:

```typescript
import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAnimatedTick } from './useAnimatedTick'

describe('useAnimatedTick', () => {
  let rafCallbacks: ((time: number) => void)[]
  let rafId: number

  beforeEach(() => {
    rafCallbacks = []
    rafId = 0
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
      rafCallbacks.push(cb)
      return ++rafId
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})
    vi.spyOn(performance, 'now').mockReturnValue(0)
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  function flushRaf(time: number) {
    vi.spyOn(performance, 'now').mockReturnValue(time)
    const cbs = [...rafCallbacks]
    rafCallbacks = []
    cbs.forEach((cb) => cb(time))
  }

  it('returns serverTick initially before any rAF fires', () => {
    const { result } = renderHook(() => useAnimatedTick(100, 10))
    expect(result.current.displayTick).toBe(100)
  })

  it('interpolates forward between server updates', () => {
    const { result, rerender } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 100, rate: 10 } },
    )

    // Simulate 100ms passing (should advance ~1 tick at 10 ticks/sec)
    act(() => { flushRaf(100) })
    expect(result.current.displayTick).toBeCloseTo(101, 0)
  })

  it('snaps to serverTick when a new server value arrives', () => {
    const { result, rerender } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 100, rate: 10 } },
    )

    // Advance 50ms
    act(() => { flushRaf(50) })

    // Server says tick is now 105
    rerender({ serverTick: 105, rate: 10 })
    act(() => { flushRaf(51) })

    expect(result.current.displayTick).toBeGreaterThanOrEqual(105)
  })

  it('measures tick rate from server samples', () => {
    const { result, rerender } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 0, rate: 10 } },
    )

    // Feed several server tick updates at 20 ticks/sec (50ms per tick)
    for (let tick = 1; tick <= 5; tick++) {
      vi.spyOn(performance, 'now').mockReturnValue(tick * 50)
      rerender({ serverTick: tick, rate: 10 })
    }

    // measuredTickRate should be close to 20
    expect(result.current.measuredTickRate).toBeGreaterThan(15)
    expect(result.current.measuredTickRate).toBeLessThan(25)
  })

  it('does not advance displayTick beyond reasonable bound', () => {
    const { result } = renderHook(() => useAnimatedTick(100, 10))

    // Simulate a huge time jump (2 seconds — way more than expected)
    act(() => { flushRaf(2000) })

    // Should be clamped, not at 100 + 20 = 120
    // Max lookahead is ~1 second worth = 10 ticks
    expect(result.current.displayTick).toBeLessThanOrEqual(115)
  })

  it('cancels rAF on unmount', () => {
    const { unmount } = renderHook(() => useAnimatedTick(100, 10))
    unmount()
    expect(window.cancelAnimationFrame).toHaveBeenCalled()
  })
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test -- --run useAnimatedTick`
Expected: FAIL — module not found.

**Step 3: Implement the hook**

Create `ui_web/src/hooks/useAnimatedTick.ts`:

```typescript
import { useEffect, useRef, useState } from 'react'

interface TickSample {
  serverTick: number
  wallTime: number // performance.now() in ms
}

const MAX_SAMPLES = 10
const MAX_LOOKAHEAD_MS = 1000 // never extrapolate more than 1s ahead

export function useAnimatedTick(serverTick: number, initialTickRate: number) {
  const [displayTick, setDisplayTick] = useState(serverTick)
  const [measuredTickRate, setMeasuredTickRate] = useState(initialTickRate)

  const samplesRef = useRef<TickSample[]>([])
  const rateRef = useRef(initialTickRate)
  const anchorRef = useRef<{ tick: number; wallTime: number }>({
    tick: serverTick,
    wallTime: performance.now(),
  })

  // Record server tick samples and compute measured rate
  useEffect(() => {
    const now = performance.now()
    const samples = samplesRef.current

    // Only record if tick actually changed
    if (samples.length === 0 || samples[samples.length - 1].serverTick !== serverTick) {
      samples.push({ serverTick, wallTime: now })
      if (samples.length > MAX_SAMPLES) samples.shift()

      // Update anchor point for interpolation
      anchorRef.current = { tick: serverTick, wallTime: now }

      // Compute measured rate from samples
      if (samples.length >= 2) {
        const oldest = samples[0]
        const newest = samples[samples.length - 1]
        const elapsedMs = newest.wallTime - oldest.wallTime
        const elapsedTicks = newest.serverTick - oldest.serverTick
        if (elapsedMs > 0 && elapsedTicks > 0) {
          const rate = (elapsedTicks / elapsedMs) * 1000
          rateRef.current = rate
          setMeasuredTickRate(rate)
        }
      }
    }
  }, [serverTick])

  // rAF loop for smooth interpolation
  useEffect(() => {
    let rafHandle: number

    function animate() {
      const now = performance.now()
      const anchor = anchorRef.current
      const elapsedMs = Math.min(now - anchor.wallTime, MAX_LOOKAHEAD_MS)
      const interpolatedTick = anchor.tick + (rateRef.current * elapsedMs) / 1000
      setDisplayTick(interpolatedTick)
      rafHandle = requestAnimationFrame(animate)
    }

    rafHandle = requestAnimationFrame(animate)
    return () => cancelAnimationFrame(rafHandle)
  }, [])

  return { displayTick, measuredTickRate }
}
```

**Step 4: Run tests to verify they pass**

Run: `cd ui_web && npm test -- --run useAnimatedTick`
Expected: All 6 tests pass.

**Step 5: Commit**

```bash
git add ui_web/src/hooks/useAnimatedTick.ts ui_web/src/hooks/useAnimatedTick.test.ts
git commit -m "feat(ui): add useAnimatedTick hook with rAF interpolation"
```

---

### Task 4: FE — wire `displayTick` into App and components

**Context:** Replace `currentTick` with `displayTick` in components that need smooth updates. Keep `currentTick` (discrete server tick) for the event feed and anywhere events are discrete. The `useAnimatedTick` hook from Task 3 is ready. App.tsx already fetches `ticksPerSec` from Task 2.

**Files:**
- Modify: `ui_web/src/App.tsx`
- Modify: `ui_web/src/components/StatusBar.tsx`
- Modify: `ui_web/src/components/SolarSystemMap.tsx`
- Modify: `ui_web/src/components/StatusBar.test.tsx` (if needed)

**Step 1: Wire `useAnimatedTick` into App.tsx**

In `ui_web/src/App.tsx`, import and use the hook, then pass `displayTick` to the right components:

```typescript
import { useAnimatedTick } from './hooks/useAnimatedTick'

export default function App() {
  const { snapshot, events, connected, currentTick, oreCompositions } = useSimStream()
  const { visible, toggle } = useVisiblePanels()
  const [ticksPerSec, setTicksPerSec] = useState(10)

  useEffect(() => {
    fetchMeta().then((meta) => setTicksPerSec(meta.ticks_per_sec)).catch(() => {})
  }, [])

  const { displayTick, measuredTickRate } = useAnimatedTick(currentTick, ticksPerSec)
```

Update the `renderPanel` function and StatusBar usage:

```typescript
  // StatusBar gets displayTick and measuredTickRate
  <StatusBar tick={displayTick} connected={connected} measuredTickRate={measuredTickRate} />

  // In renderPanel:
  case 'map':
    return (
      <SolarSystemMap snapshot={snapshot} currentTick={displayTick} oreCompositions={oreCompositions} />
    )
  case 'fleet':
    return (
      <FleetPanel
        ships={snapshot?.ships ?? {}}
        stations={snapshot?.stations ?? {}}
        oreCompositions={oreCompositions}
        displayTick={displayTick}
      />
    )
```

**Step 2: Update StatusBar to show `measuredTickRate` and accept float tick**

In `ui_web/src/components/StatusBar.tsx`:

```typescript
interface Props {
  tick: number
  connected: boolean
  measuredTickRate: number
}

export function StatusBar({ tick, connected, measuredTickRate }: Props) {
  const roundedTick = Math.floor(tick)
  const day = Math.floor(roundedTick / 1440)
  const hour = Math.floor((roundedTick % 1440) / 60)
  const minute = roundedTick % 60

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-surface border-b border-edge text-xs shrink-0">
      <span className="text-accent font-bold">tick {roundedTick}</span>
      <span className="text-dim">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className="text-muted">~{measuredTickRate.toFixed(1)} t/s</span>
      <span className={connected ? 'text-online' : 'text-offline'}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
    </div>
  )
}
```

**Step 3: SolarSystemMap already uses `currentTick` as a number — no changes needed**

The prop is `currentTick: number` and it's already used for interpolation math. Passing a float works — `(displayTick - started_tick) / (eta_tick - started_tick)` gives smooth sub-tick progress.

**Step 4: Update StatusBar tests if needed**

In `ui_web/src/components/StatusBar.test.tsx`, if tests pass a `tick` prop, they'll still work since `Math.floor` of an integer is the same integer. If the test checks for the `measuredTickRate` text, add the prop. Check what needs updating.

**Step 5: Run tests**

Run: `cd ui_web && npm test -- --run`
Expected: All tests pass. May need to update test fixtures to include `measuredTickRate` prop.

**Step 6: Commit**

```bash
git add ui_web/src/App.tsx ui_web/src/components/StatusBar.tsx ui_web/src/components/StatusBar.test.tsx
git commit -m "feat(ui): wire displayTick into App, StatusBar, and SolarSystemMap"
```

---

### Task 5: FE — add progress bars to FleetPanel

**Context:** FleetPanel currently shows task type as a text label ("survey", "mine", etc.) with no progress indication. Add a `displayTick` prop and render a progress bar for active tasks using `(displayTick - started_tick) / (eta_tick - started_tick)`.

**Files:**
- Modify: `ui_web/src/components/FleetPanel.tsx`
- Modify: `ui_web/src/components/FleetPanel.test.tsx`

**Step 1: Write failing test for progress bar**

In `ui_web/src/components/FleetPanel.test.tsx`, add a test that checks for a progress bar element when a ship has an active task:

```typescript
it('shows progress bar for active task', () => {
  const ships: Record<string, ShipState> = {
    ship_0001: {
      id: 'ship_0001',
      location_node: 'node_earth_orbit',
      owner: 'principal_autopilot',
      cargo: {},
      cargo_capacity_m3: 20,
      task: {
        kind: { Mine: { asteroid: 'asteroid_0001', duration_ticks: 100 } },
        started_tick: 0,
        eta_tick: 100,
      },
    },
  }
  render(
    <FleetPanel ships={ships} stations={{}} oreCompositions={{}} displayTick={50} />,
  )
  const progressBar = document.querySelector('[role="progressbar"]')
  expect(progressBar).toBeInTheDocument()
  expect(progressBar?.getAttribute('aria-valuenow')).toBe('50')
})

it('shows no progress bar for idle ship', () => {
  const ships: Record<string, ShipState> = {
    ship_0001: {
      id: 'ship_0001',
      location_node: 'node_earth_orbit',
      owner: 'principal_autopilot',
      cargo: {},
      cargo_capacity_m3: 20,
      task: null,
    },
  }
  render(
    <FleetPanel ships={ships} stations={{}} oreCompositions={{}} displayTick={50} />,
  )
  expect(document.querySelector('[role="progressbar"]')).not.toBeInTheDocument()
})
```

**Step 2: Run test to verify it fails**

Run: `cd ui_web && npm test -- --run FleetPanel`
Expected: FAIL — `displayTick` prop not recognized / no progressbar element.

**Step 3: Add `displayTick` prop and progress bar to FleetPanel**

In `ui_web/src/components/FleetPanel.tsx`:

Update `Props`:

```typescript
interface Props {
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  oreCompositions: OreCompositions
  displayTick: number
}
```

Add a `TaskProgress` component inside the file:

```typescript
function TaskProgress({ task, displayTick }: { task: ShipState['task']; displayTick: number }) {
  if (!task) return null
  const total = task.eta_tick - task.started_tick
  if (total <= 0) return null
  const elapsed = Math.max(0, Math.min(displayTick - task.started_tick, total))
  const pctDone = Math.round((elapsed / total) * 100)

  return (
    <div className="flex items-center gap-1.5 min-w-[80px]">
      <div
        role="progressbar"
        aria-valuenow={pctDone}
        aria-valuemin={0}
        aria-valuemax={100}
        className="flex-1 h-1.5 bg-edge rounded-full overflow-hidden"
      >
        <div
          className="h-full bg-accent rounded-full"
          style={{ width: `${pctDone}%` }}
        />
      </div>
      <span className="text-muted text-[10px] w-7 text-right">{pctDone}%</span>
    </div>
  )
}
```

Update the ships table to add a Progress column header and cell:

In the `<thead>` after the Task header:

```tsx
<th className={headerClass}>Progress</th>
```

In the `<tbody>` row after the task label cell:

```tsx
<td className="px-2 py-0.5 border-b border-surface">
  <TaskProgress task={ship.task} displayTick={displayTick} />
</td>
```

Thread `displayTick` through: `FleetPanel` → `ShipsTable` → row rendering.

**Step 4: Run tests**

Run: `cd ui_web && npm test -- --run`
Expected: All tests pass including new FleetPanel progress bar tests.

**Step 5: Commit**

```bash
git add ui_web/src/components/FleetPanel.tsx ui_web/src/components/FleetPanel.test.tsx
git commit -m "feat(ui): add task progress bars to FleetPanel"
```

---

### Task 6: Update FE watchdog timeout

**Context:** The `useSimStream` hook has a `WATCHDOG_MS = 10_000` constant that triggers a reset if no data arrives within 10 seconds. The comment says "Must be longer than heartbeat interval (5s) with margin". With heartbeat now at 200ms, the watchdog can be shorter. But we should keep it generous to avoid false disconnects. Lower it to 3 seconds.

**Files:**
- Modify: `ui_web/src/hooks/useSimStream.ts`
- Modify: `ui_web/src/hooks/useSimStream.test.ts`

**Step 1: Update the constant and comment**

In `ui_web/src/hooks/useSimStream.ts`, update:

```typescript
// Must be longer than heartbeat interval (200ms) with generous margin
const WATCHDOG_MS = 3_000
```

**Step 2: Update the watchdog test**

In `ui_web/src/hooks/useSimStream.test.ts`, the watchdog test advances time by `11_000`. Update it to use `3_100` (just over the new 3s watchdog):

```typescript
// Advance past watchdog timeout with no messages
await act(async () => { vi.advanceTimersByTime(3_100) })
```

**Step 3: Run tests**

Run: `cd ui_web && npm test -- --run`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add ui_web/src/hooks/useSimStream.ts ui_web/src/hooks/useSimStream.test.ts
git commit -m "feat(ui): reduce watchdog timeout to 3s to match faster heartbeat"
```

---

### Task 7: Final verification

**Files:** None — verification only.

**Step 1: Run all Rust tests**

Run: `cargo test`
Expected: All tests pass.

**Step 2: Run all FE tests**

Run: `cd ui_web && npm test -- --run`
Expected: All tests pass.

**Step 3: Build FE**

Run: `cd ui_web && npm run build`
Expected: Clean build, no errors.

**Step 4: Manual smoke test (optional)**

Start daemon and FE:
```bash
cargo run -p sim_daemon -- run --seed 42 --ticks-per-sec 50 &
cd ui_web && npm run dev
```

Verify:
- Tick counter updates smoothly (not jumping)
- Ships move smoothly on the solar system map during transit
- Fleet panel shows progress bars that fill smoothly
- `~XX.X t/s` readout in status bar shows a reasonable value near 50
- Connection indicator is green
