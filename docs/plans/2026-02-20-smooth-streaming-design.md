# Smooth Streaming Design

## Goal

Make the FE feel like a real-time game — smooth 60fps ship movement, continuously updating tick counter, task progress bars — instead of the current jittery discrete-tick updates.

## Problem

The FE only updates when SSE events or heartbeats arrive. Many ticks produce no events, so the tick counter and ship positions jump in chunks. At 50 ticks/sec, jumps of 100-200 ticks are common during quiet periods (5s heartbeat interval).

## Approach: Client-side interpolation via `useAnimatedTick`

Standard game networking pattern: the FE maintains a continuously advancing "display tick" driven by `requestAnimationFrame`, estimated from measured server tick rate. Server corrections arrive frequently (200ms heartbeats) to prevent drift.

## Backend Changes

1. **`/api/v1/meta` adds `ticks_per_sec`** — the configured tick rate. FE uses this as initial estimate before it has enough observed samples.
2. **Heartbeat interval drops from 5s to 200ms** — provides frequent tick corrections. Message format unchanged: `{"heartbeat": true, "tick": N}`.
3. No changes to event format, snapshot, or flush interval (stays at 50ms).

## `useAnimatedTick` Hook

Takes `serverTick` (from `useSimStream`) and initial `ticksPerSec` (from `/meta`). Outputs `displayTick` and `measuredTickRate`.

**Clock estimation:**
- Rolling window of ~10 recent `(serverTick, performance.now())` samples.
- On each new server tick, pushes a sample and computes `measuredRate = (newestTick - oldestTick) / (newestTime - oldestTime)`.
- Falls back to configured rate until enough samples exist.

**Animation loop:**
- `requestAnimationFrame` loop runs continuously.
- Each frame: `displayTick = lastServerTick + measuredRate * (now - lastServerTime)`.
- Clamped to prevent overshooting during lag.
- Snaps to server tick on each correction (no lerp — at 200ms corrections the jump is imperceptible).

**Output:**
- `displayTick: number` — floating point, advances at ~60fps.
- `measuredTickRate: number` — current estimated ticks/sec.

## Component Changes

| Component | Change |
|---|---|
| StatusBar | Display `displayTick` instead of `currentTick`. Add `measuredTickRate` readout (`~48.2 t/s`) in dim text for debug. |
| SolarSystemMap | Swap `currentTick` → `displayTick`. Ship transit positions become 60fps smooth for free. |
| FleetPanel | Add progress bar column for active tasks: `(displayTick - started_tick) / (eta_tick - started_tick)`. Idle ships show no bar. |
| App.tsx | Fetch `/meta` on mount for `ticks_per_sec`. Wire `useAnimatedTick(serverTick, ticksPerSec)`. Pass `displayTick` downstream. |
| EventsFeed | Unchanged — driven by discrete events. |
| AsteroidTable | Unchanged — driven by discrete events. |
| ResearchPanel | Unchanged — driven by discrete events. |

## What Stays Discrete vs. Smooth

**Discrete (event-driven):** cargo amounts, asteroid discoveries, tech unlocks, event feed entries. Update when events arrive.

**Smooth (tick-interpolated):** tick counter, ship transit positions, task progress bars, ticks/sec debug readout.

## Data Flow

```
/api/v1/meta (on mount)
  └─ ticks_per_sec → initial estimate for useAnimatedTick

useSimStream (existing, unchanged)
  └─ serverTick, snapshot, events, connected, oreCompositions

useAnimatedTick(serverTick, ticksPerSec)
  ├─ rAF loop → displayTick (float, 60fps)
  ├─ rolling window of (serverTick, wallTime) → measuredTickRate
  └─ snaps to serverTick on each correction

App.tsx
  ├─ StatusBar ← displayTick, measuredTickRate, connected
  ├─ SolarSystemMap ← displayTick, snapshot
  ├─ FleetPanel ← displayTick, snapshot (new progress bars)
  ├─ AsteroidTable ← snapshot (unchanged)
  ├─ ResearchPanel ← snapshot (unchanged)
  └─ EventsFeed ← events (unchanged)
```
