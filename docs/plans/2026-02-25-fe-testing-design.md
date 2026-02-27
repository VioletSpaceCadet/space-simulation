# FE Testing System Design

**Date:** 2026-02-25
**Status:** Approved

## Goals

1. **E2E tests** — Automated Playwright tests that start the Rust daemon + Vite dev server and verify core UI flows end-to-end.
2. **CC screenshot tool** — An MCP server (+ CLI wrapper) that lets Claude Code take screenshots of the running app during FE development, providing visual signal without writing automated tests for everything.

## Project Structure

Single `e2e/` package at the repo root:

```
e2e/
├── package.json              # playwright, @modelcontextprotocol/sdk, tsx
├── tsconfig.json
├── playwright.config.ts      # baseURL, globalSetup/Teardown, projects
├── global-setup.ts           # Start daemon + vite dev server, wait for health
├── global-teardown.ts        # Kill daemon + vite
├── tests/
│   ├── app-loads.spec.ts     # SSE connects, tick counter increments
│   ├── pause-resume.spec.ts  # Button + spacebar toggle
│   ├── import-command.spec.ts # Economy panel import flow
│   ├── save-game.spec.ts     # Save button + Cmd+S
│   └── speed-controls.spec.ts # 1-5 keyboard presets
├── mcp-server.ts             # MCP server exposing screenshot + navigate tools
├── screenshot-cli.ts         # CLI: npx tsx screenshot-cli.ts [url] [output]
└── lib/
    └── browser.ts            # Shared: launch browser, create page, screenshot
```

## E2E Test Infrastructure

### Ports

E2E tests use dedicated ports to avoid colliding with dev instances:

| Service | Dev | E2E |
|---------|-----|-----|
| sim_daemon | 3001 | 3002 |
| Vite dev server | 5173 | 5174 |

Vite E2E instance proxies `/api` to `localhost:3002`.

### Playwright Config

- Base URL: `http://localhost:5174`
- Single project: Chromium only
- Retries: 1 in CI, 0 locally
- Screenshots on failure: `e2e/test-results/`

### Global Setup

1. `cargo build -p sim_daemon` (fail fast on compile error)
2. Spawn `cargo run -p sim_daemon -- run --seed 42 --paused --port 3002`
3. Poll `http://localhost:3002/api/v1/meta` until 200 (timeout 30s)
4. Spawn `npm run dev -- --port 5174` in `ui_web/` (proxy configured to 3002)
5. Poll `http://localhost:5174` until 200 (timeout 15s)
6. Store child process PIDs for teardown

The `--paused` flag starts the daemon in a paused state so tests control timing. Tests resume and set speed via `/api/v1/resume` and `/api/v1/speed` as needed.

### Global Teardown

Kill both child processes (SIGTERM, then SIGKILL after 5s).

### CI Integration

- New `scripts/ci_e2e.sh` script
- New GitHub Actions job that builds Rust first, then runs `npx playwright test`
- Separate from existing web/rust CI jobs since it requires both runtimes

## MCP Server

**Transport:** stdio

**Tools:**

| Tool | Params | Returns |
|------|--------|---------|
| `screenshot` | `path?` (default `/`), `width?` (1280), `height?` (720), `fullPage?` (false) | Base64 PNG as image content block |
| `navigate` | `path` | Page title, URL, load success boolean |

**Browser lifecycle:**
- Lazy-init on first tool call, reuses across calls
- Single browser instance, single page
- Closes on server shutdown
- Assumes dev server already running at `localhost:5173` (user's dev instance)

**Claude Code config** (`.claude/mcp.json`):
```json
{
  "mcpServers": {
    "playwright": {
      "command": "npx",
      "args": ["tsx", "e2e/mcp-server.ts"],
      "cwd": "/absolute/path/to/space-simulation"
    }
  }
}
```

### CLI Wrapper

`npx tsx e2e/screenshot-cli.ts /economy --output /tmp/economy.png`

Same browser logic, writes PNG to file.

## Core E2E Tests

### 1. app-loads.spec.ts
- Navigate to `/`
- Wait for SSE connection (status bar shows "Running" or tick > 0)
- Assert tick counter visible and incrementing
- Assert at least one panel rendered

### 2. pause-resume.spec.ts
- Wait for tick to start
- Click pause → assert "Paused", tick stops
- Click resume → tick resumes
- Spacebar pause → assert paused
- Spacebar resume → assert resumed

### 3. import-command.spec.ts
- Wait for app to load
- Open Economy panel
- Select Material > Iron, set quantity
- Click Import
- Assert balance decreases

### 4. save-game.spec.ts
- Wait for app to load
- Click save button → assert success feedback
- Test Cmd+S triggers same behavior

### 5. speed-controls.spec.ts
- Wait for app to load
- Press 1 → assert ~100 TPS displayed
- Press 5 → assert max TPS
- Press 2 → assert ~1K TPS

## Future E2E Test Candidates (backlog)

- Panel drag-and-drop rearrangement
- Research panel tech tree interaction
- Fleet panel expandable rows + detail sections
- Alert badges appear and are dismissible
- Solar system map zoom/pan
- Asteroid table updates as ships mine
- Full gameplay loop: mine → refine → assemble → build ship
