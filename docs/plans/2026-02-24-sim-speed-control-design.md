# Sim Speed Control

## Problem

Tick rate is fixed at startup via `--ticks-per-sec` CLI flag. No way to change speed at runtime from the UI.

## Changes

### Backend (sim_daemon)

- Replace fixed `tokio::time::interval` in tick loop with dynamic per-tick sleep based on a shared atomic tick rate value
- New endpoint: `POST /api/v1/speed` with body `{"ticks_per_sec": N}` (0 = unlimited)
- `/api/v1/meta` already returns `ticks_per_sec` — will reflect runtime value

### Frontend (ui_web)

- 5 speed buttons in StatusBar next to pause/save: `100`, `1K`, `10K`, `100K`, `Max`
- Active speed highlighted
- Keyboard shortcuts: 1-5 (regular + numpad) map to the 5 speeds
- Calls `POST /api/v1/speed` on click or keypress

### What doesn't change

- Pause/resume — orthogonal
- Metrics collection interval — stays at N ticks
- Alert evaluation — still fires on metrics samples
- Save — works at any speed
