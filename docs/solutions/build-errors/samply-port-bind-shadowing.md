---
title: "samply profiler shadows sim_daemon via specific-bind on macOS"
category: build-errors
date: 2026-04-12
tags:
  - samply
  - port-conflict
  - macos
  - sim_daemon
  - localhost
severity: low
components:
  - sim_daemon
  - samply
---

## Symptom

The ui_web frontend shows "reconnecting..." and `/api/v1/snapshot` returns 404.
Navigating directly to `http://localhost:3001` in the browser shows the
**samply profiler symbol server** page instead of the sim_daemon API.

sim_daemon's terminal output appears normal — it prints
`listening on http://localhost:3001  speed=10 ticks/sec` and even produces
tick-loop logs. But no HTTP requests from the UI reach it.

## Root Cause

Both `samply` and `sim_daemon` bind to port 3001, but to DIFFERENT addresses:

| Process | Bind address | Scope |
|---------|-------------|-------|
| `samply` (profiler) | `127.0.0.1:3001` (specific loopback) | First, specific |
| `sim_daemon` | `0.0.0.0:3001` (wildcard all interfaces) | Second, wildcard |

macOS allows both to succeed because:
- `samply` binds to the **specific** address `127.0.0.1:3001`
- `sim_daemon` binds to the **wildcard** `0.0.0.0:3001`
- The kernel's routing prefers the specific bind: any incoming request to
  `127.0.0.1:3001` (which is how `localhost` resolves) goes to samply

sim_daemon only receives requests arriving on non-loopback interfaces (which
in local dev is never).

## Diagnosis

```bash
lsof -i :3001 -Pn
```

Shows both processes:
```
samply     4938  ...  TCP 127.0.0.1:3001 (LISTEN)    ← specific
sim_daemo 25724  ...  TCP *:3001         (LISTEN)    ← wildcard
```

## Solution

Kill the stale `samply` process:

```bash
kill <samply_pid>
```

Then verify only sim_daemon remains:
```bash
lsof -i :3001 -Pn
# Should show only sim_daemo on *:3001
```

The UI should reconnect automatically after a few seconds.

## Prevention

- After running `samply record ...` for CPU profiling, always close the
  samply process (Ctrl+C in its terminal, or `kill`). The symbol server
  stays alive in the background otherwise.
- Before starting `sim_daemon`, check `lsof -i :3001 -Pn` to see if the
  port is already taken. If samply or another process holds it, kill it first.
- Consider adding a startup preflight check to `sim_daemon` that warns if
  the specific loopback address is already bound by another process.
