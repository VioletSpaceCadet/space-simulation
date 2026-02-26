# FE Testing System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Playwright E2E tests and an MCP screenshot server so Claude Code can verify UI behavior and take screenshots during FE development.

**Architecture:** Single `e2e/` package at repo root with Playwright for E2E tests + an MCP server (stdio transport) using shared browser helpers. Global setup spawns sim_daemon (port 3002, paused) and Vite dev server (port 5174). Tests control timing via API.

**Tech Stack:** Playwright, TypeScript, @modelcontextprotocol/sdk, tsx

---

### Task 1: Add `--paused` CLI flag to sim_daemon (VIO-183)

**Files:**
- Modify: `crates/sim_daemon/src/main.rs`

**Step 1: Add the `--paused` arg to the `Run` variant**

In the `Commands` enum `Run` variant (around line 55, after the `cors_origin` field), add:

```rust
        /// Start the simulation in a paused state.
        #[arg(long)]
        paused: bool,
```

**Step 2: Wire it into AppState construction**

In the `Commands::Run` match arm (around line 135), change:

```rust
                paused: Arc::new(AtomicBool::new(false)),
```

to:

```rust
                paused: Arc::new(AtomicBool::new(paused)),
```

**Step 3: Add test**

In the `#[cfg(test)] mod tests` block in `main.rs`, add:

```rust
    #[test]
    fn test_paused_flag_parsed() {
        use clap::Parser;
        let cli = Cli::parse_from(["sim_daemon", "run", "--seed", "1", "--paused"]);
        match cli.command {
            Commands::Run { paused, .. } => assert!(paused),
        }
    }
```

**Step 4: Run tests**

Run: `cargo test -p sim_daemon test_paused_flag_parsed`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/sim_daemon/src/main.rs
git commit -m "feat(sim_daemon): add --paused CLI flag

Start simulation in paused state. Tests control timing via
/api/v1/resume and /api/v1/speed endpoints.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 2: Scaffold e2e/ package (VIO-140)

**Files:**
- Create: `e2e/package.json`
- Create: `e2e/tsconfig.json`
- Create: `e2e/playwright.config.ts`
- Create: `e2e/.gitignore`
- Modify: `.gitignore` (add `e2e/node_modules/`, `e2e/test-results/`)

**Step 1: Create `e2e/package.json`**

```json
{
  "name": "space-sim-e2e",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "test": "playwright test",
    "test:headed": "playwright test --headed",
    "screenshot": "tsx screenshot-cli.ts",
    "mcp": "tsx mcp-server.ts"
  },
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.12.1",
    "zod": "^3.23.0"
  },
  "devDependencies": {
    "@playwright/test": "^1.52.0",
    "@types/node": "^22.0.0",
    "tsx": "^4.19.0",
    "typescript": "~5.7.0"
  }
}
```

**Step 2: Create `e2e/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "Node16",
    "moduleResolution": "Node16",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "dist",
    "rootDir": "."
  },
  "include": ["*.ts", "lib/**/*.ts", "tests/**/*.ts"]
}
```

**Step 3: Create `e2e/playwright.config.ts`**

```typescript
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  use: {
    baseURL: "http://localhost:5174",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
  outputDir: "./test-results",
  globalSetup: "./global-setup.ts",
  globalTeardown: "./global-teardown.ts",
});
```

**Step 4: Create `e2e/.gitignore`**

```
node_modules/
test-results/
dist/
playwright-report/
```

**Step 5: Update root `.gitignore`**

Append:

```
# E2E tests
e2e/node_modules/
e2e/test-results/
e2e/playwright-report/
```

**Step 6: Install dependencies**

Run: `cd e2e && npm install && npx playwright install chromium`

**Step 7: Verify TypeScript compiles**

Run: `cd e2e && npx tsc --noEmit`
Expected: No errors (may warn about missing files referenced in config — that's fine, they'll be created in later tasks)

**Step 8: Commit**

```bash
git add e2e/package.json e2e/package-lock.json e2e/tsconfig.json e2e/playwright.config.ts e2e/.gitignore .gitignore
git commit -m "feat(e2e): scaffold Playwright package with config

Separate e2e/ package at repo root. Chromium-only, port 5174,
screenshots on failure. Global setup/teardown stubs referenced.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 3: Global setup and teardown (VIO-141)

**Files:**
- Create: `e2e/global-setup.ts`
- Create: `e2e/global-teardown.ts`

**Step 1: Create `e2e/global-setup.ts`**

```typescript
import { spawn, execSync, type ChildProcess } from "node:child_process";
import path from "node:path";
import os from "node:os";
import fs from "node:fs";

const PROJECT_ROOT = path.resolve(import.meta.dirname, "..");
const DAEMON_PORT = 3002;
const VITE_PORT = 5174;
const PID_FILE = path.join(os.tmpdir(), "space-sim-e2e-pids.json");

function findCargo(): string {
  const home = os.homedir();
  const cargoPath = path.join(home, ".cargo", "bin", "cargo");
  if (fs.existsSync(cargoPath)) return cargoPath;
  return "cargo";
}

async function waitForUrl(
  url: string,
  retries: number,
  intervalMs: number,
): Promise<void> {
  for (let attempt = 0; attempt < retries; attempt++) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // not ready
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

export default async function globalSetup(): Promise<void> {
  const cargo = findCargo();

  // Build first to fail fast on compile errors
  console.log("[e2e] Building sim_daemon...");
  execSync(`${cargo} build -p sim_daemon`, {
    cwd: PROJECT_ROOT,
    stdio: "inherit",
  });

  // Start daemon
  console.log("[e2e] Starting sim_daemon on port", DAEMON_PORT);
  const daemon = spawn(
    cargo,
    [
      "run",
      "-p",
      "sim_daemon",
      "--",
      "run",
      "--seed",
      "42",
      "--paused",
      "--port",
      String(DAEMON_PORT),
      "--cors-origin",
      `http://localhost:${VITE_PORT}`,
    ],
    { cwd: PROJECT_ROOT, stdio: "ignore", detached: false },
  );

  // Start Vite dev server
  console.log("[e2e] Starting Vite dev server on port", VITE_PORT);
  const vite = spawn(
    "npx",
    ["vite", "--port", String(VITE_PORT)],
    {
      cwd: path.join(PROJECT_ROOT, "ui_web"),
      stdio: "ignore",
      detached: false,
      env: {
        ...process.env,
        VITE_API_TARGET: `http://localhost:${DAEMON_PORT}`,
      },
    },
  );

  // Save PIDs for teardown
  fs.writeFileSync(
    PID_FILE,
    JSON.stringify({ daemon: daemon.pid, vite: vite.pid }),
  );

  // Wait for both to be ready
  console.log("[e2e] Waiting for daemon...");
  await waitForUrl(
    `http://localhost:${DAEMON_PORT}/api/v1/meta`,
    60,
    500,
  );

  console.log("[e2e] Waiting for Vite...");
  await waitForUrl(`http://localhost:${VITE_PORT}`, 30, 500);

  console.log("[e2e] Setup complete. Daemon and Vite are running.");
}
```

**Step 2: Create `e2e/global-teardown.ts`**

```typescript
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const PID_FILE = path.join(os.tmpdir(), "space-sim-e2e-pids.json");

export default async function globalTeardown(): Promise<void> {
  if (!fs.existsSync(PID_FILE)) {
    console.log("[e2e] No PID file found, skipping teardown.");
    return;
  }

  const pids = JSON.parse(fs.readFileSync(PID_FILE, "utf-8")) as {
    daemon?: number;
    vite?: number;
  };
  fs.unlinkSync(PID_FILE);

  for (const [name, pid] of Object.entries(pids)) {
    if (pid == null) continue;
    try {
      process.kill(pid, "SIGTERM");
      console.log(`[e2e] Sent SIGTERM to ${name} (pid ${pid})`);
    } catch {
      // Process already exited
    }
  }

  // Give processes a moment to exit
  await new Promise((resolve) => setTimeout(resolve, 1000));

  // Force kill if still running
  for (const [name, pid] of Object.entries(pids)) {
    if (pid == null) continue;
    try {
      process.kill(pid, 0); // check if alive
      process.kill(pid, "SIGKILL");
      console.log(`[e2e] Force-killed ${name} (pid ${pid})`);
    } catch {
      // Already dead, good
    }
  }
}
```

**Step 3: Handle Vite proxy for E2E port**

The Vite config in `ui_web/vite.config.ts` hardcodes proxy target to `localhost:3001`. For E2E tests, we need it to proxy to `localhost:3002`. Update global-setup to pass the target via env var.

Modify `ui_web/vite.config.ts` — change the proxy target to read from env:

```typescript
// In the server.proxy config, change:
target: 'http://localhost:3001',
// To:
target: process.env.VITE_API_TARGET ?? 'http://localhost:3001',
```

**Step 4: Verify TypeScript compiles**

Run: `cd e2e && npx tsc --noEmit`
Expected: No errors

**Step 5: Commit**

```bash
git add e2e/global-setup.ts e2e/global-teardown.ts ui_web/vite.config.ts
git commit -m "feat(e2e): global setup/teardown for daemon + Vite

Spawns sim_daemon --paused --port 3002 and Vite on 5174. Polls
health endpoints for readiness. Teardown sends SIGTERM then SIGKILL.
Vite proxy target now configurable via VITE_API_TARGET env var.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 4: Shared browser helpers (VIO-142)

**Files:**
- Create: `e2e/lib/browser.ts`

**Step 1: Create `e2e/lib/browser.ts`**

```typescript
import { chromium, type Browser, type Page } from "playwright";

const BASE_URL = process.env.BASE_URL ?? "http://localhost:5173";

let browser: Browser | null = null;
let page: Page | null = null;

export async function launchBrowser(): Promise<Browser> {
  if (browser) return browser;
  browser = await chromium.launch({ headless: true });
  return browser;
}

export async function getPage(): Promise<Page> {
  if (page && !page.isClosed()) return page;
  const b = await launchBrowser();
  const context = await b.newContext({
    viewport: { width: 1280, height: 720 },
  });
  page = await context.newPage();
  return page;
}

export interface ScreenshotOptions {
  path?: string;
  width?: number;
  height?: number;
  fullPage?: boolean;
}

export async function takeScreenshot(
  targetPage: Page,
  options: ScreenshotOptions = {},
): Promise<Buffer> {
  const {
    width = 1280,
    height = 720,
    fullPage = false,
  } = options;
  await targetPage.setViewportSize({ width, height });
  return targetPage.screenshot({ fullPage, type: "png" });
}

export async function navigateTo(
  targetPage: Page,
  urlPath: string,
): Promise<{ title: string; url: string; success: boolean }> {
  const url = urlPath.startsWith("http")
    ? urlPath
    : `${BASE_URL}${urlPath.startsWith("/") ? urlPath : `/${urlPath}`}`;
  try {
    await targetPage.goto(url, { waitUntil: "networkidle" });
    return {
      title: await targetPage.title(),
      url: targetPage.url(),
      success: true,
    };
  } catch {
    return {
      title: "",
      url,
      success: false,
    };
  }
}

export async function closeBrowser(): Promise<void> {
  if (browser) {
    await browser.close();
    browser = null;
    page = null;
  }
}
```

**Step 2: Verify it compiles**

Run: `cd e2e && npx tsc --noEmit`
Expected: No errors

**Step 3: Commit**

```bash
git add e2e/lib/browser.ts
git commit -m "feat(e2e): shared browser helpers for MCP and CLI

Launch/close browser, take screenshots with viewport resize,
navigate with error handling. Lazy-init singleton pattern.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 5: MCP screenshot server (VIO-143)

**Files:**
- Create: `e2e/mcp-server.ts`
- Modify: `.mcp.json` (add playwright entry)

**Step 1: Create `e2e/mcp-server.ts`**

```typescript
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import {
  getPage,
  takeScreenshot,
  navigateTo,
  closeBrowser,
} from "./lib/browser.js";

const server = new McpServer({
  name: "playwright-screenshot",
  version: "0.1.0",
});

server.tool(
  "screenshot",
  "Take a screenshot of the running app. Returns a base64 PNG image.",
  {
    path: z.string().optional().describe("URL path (default '/')"),
    width: z.number().int().optional().describe("Viewport width (default 1280)"),
    height: z.number().int().optional().describe("Viewport height (default 720)"),
    fullPage: z.boolean().optional().describe("Capture full page (default false)"),
  },
  async ({ path: urlPath, width, height, fullPage }) => {
    const page = await getPage();
    await navigateTo(page, urlPath ?? "/");
    // Wait for app to hydrate
    await page.waitForTimeout(1000);

    const buffer = await takeScreenshot(page, { width, height, fullPage });

    return {
      content: [
        {
          type: "image" as const,
          data: buffer.toString("base64"),
          mimeType: "image/png",
        },
      ],
    };
  },
);

server.tool(
  "navigate",
  "Navigate to a page in the app and return page info.",
  {
    path: z.string().describe("URL path to navigate to"),
  },
  async ({ path: urlPath }) => {
    const page = await getPage();
    const result = await navigateTo(page, urlPath);

    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result),
        },
      ],
    };
  },
);

// Cleanup on exit
process.on("exit", () => {
  void closeBrowser();
});

const transport = new StdioServerTransport();
await server.connect(transport);
```

**Step 2: Add to `.mcp.json`**

Add the `playwright` entry to the existing `.mcp.json`:

```json
{
  "mcpServers": {
    "balance-advisor": {
      "command": "node",
      "args": ["mcp_advisor/dist/index.js"],
      "env": {
        "DAEMON_URL": "http://localhost:3001",
        "CONTENT_DIR": "./content"
      }
    },
    "playwright": {
      "command": "npx",
      "args": ["tsx", "e2e/mcp-server.ts"],
      "env": {
        "BASE_URL": "http://localhost:5173"
      }
    }
  }
}
```

**Step 3: Verify it compiles**

Run: `cd e2e && npx tsc --noEmit`
Expected: No errors

**Step 4: Commit**

```bash
git add e2e/mcp-server.ts .mcp.json
git commit -m "feat(e2e): MCP screenshot server with navigate tool

Playwright-backed MCP server for Claude Code. screenshot returns
base64 PNG image, navigate returns page title/URL/success.
Auto-discovered via .mcp.json.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 6: Screenshot CLI wrapper (VIO-144)

**Files:**
- Create: `e2e/screenshot-cli.ts`

**Step 1: Create `e2e/screenshot-cli.ts`**

```typescript
import fs from "node:fs";
import {
  getPage,
  takeScreenshot,
  navigateTo,
  closeBrowser,
} from "./lib/browser.js";

const args = process.argv.slice(2);
const urlPath = args[0] ?? "/";
const outputIndex = args.indexOf("--output");
const output = outputIndex >= 0 ? args[outputIndex + 1] : "./screenshot.png";
const widthIndex = args.indexOf("--width");
const width = widthIndex >= 0 ? parseInt(args[widthIndex + 1], 10) : 1280;
const heightIndex = args.indexOf("--height");
const height = heightIndex >= 0 ? parseInt(args[heightIndex + 1], 10) : 720;

const page = await getPage();
await navigateTo(page, urlPath);
await page.waitForTimeout(1000);
const buffer = await takeScreenshot(page, { width, height });
fs.writeFileSync(output, buffer);
console.log(`Screenshot saved to ${output}`);
await closeBrowser();
```

**Step 2: Verify it compiles**

Run: `cd e2e && npx tsc --noEmit`
Expected: No errors

**Step 3: Commit**

```bash
git add e2e/screenshot-cli.ts
git commit -m "feat(e2e): screenshot CLI wrapper

Usage: npx tsx e2e/screenshot-cli.ts [path] --output [file.png]
Thin wrapper around shared browser helpers.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 7: E2E test — app loads and displays live data (VIO-145)

**Files:**
- Create: `e2e/tests/app-loads.spec.ts`

**Step 1: Create the test file**

```typescript
import { test, expect } from "@playwright/test";

test.describe("App loads and displays live data", () => {
  test.beforeEach(async ({ page }) => {
    // Resume the sim so ticks advance
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
  });

  test.afterEach(async () => {
    // Re-pause for other tests
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("tick counter is visible and incrementing", async ({ page }) => {
    // Wait for tick to appear (contains "tick" text with a number)
    const tickText = page.locator("text=/tick \\d+/");
    await expect(tickText).toBeVisible({ timeout: 10_000 });

    // Get initial tick value
    const initialText = await tickText.textContent();
    const initialTick = parseInt(initialText!.match(/tick (\d+)/)![1], 10);

    // Wait a moment and check tick has advanced
    await page.waitForTimeout(2000);
    const laterText = await tickText.textContent();
    const laterTick = parseInt(laterText!.match(/tick (\d+)/)![1], 10);

    expect(laterTick).toBeGreaterThan(initialTick);
  });

  test("at least one panel is rendered", async ({ page }) => {
    // Wait for any nav button to be visible (panel toggle buttons)
    const navButtons = page.locator("nav button");
    await expect(navButtons.first()).toBeVisible({ timeout: 10_000 });
    const count = await navButtons.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test("status bar shows connection state", async ({ page }) => {
    // Should show "connected" or "Running" indicating SSE is working
    const statusText = page.locator("text=/connected|Running/i");
    await expect(statusText.first()).toBeVisible({ timeout: 10_000 });
  });
});
```

**Step 2: Run the test (requires daemon + vite running)**

Run: `cd e2e && npx playwright test tests/app-loads.spec.ts`
Expected: PASS (3 tests)

**Step 3: Commit**

```bash
git add e2e/tests/app-loads.spec.ts
git commit -m "test(e2e): app loads and displays live data

Smoke test: tick counter visible and incrementing, panels rendered,
SSE connection established.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 8: E2E test — pause/resume (VIO-146)

**Files:**
- Create: `e2e/tests/pause-resume.spec.ts`

**Step 1: Create the test file**

```typescript
import { test, expect } from "@playwright/test";

test.describe("Pause and resume", () => {
  test.beforeEach(async ({ page }) => {
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
    // Wait for app to be ready
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("pause button stops tick counter", async ({ page }) => {
    // Click pause
    const pauseButton = page.locator("button", { hasText: /running/i });
    await pauseButton.click();

    // Should show "Paused"
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Record tick, wait, verify it hasn't changed
    const tickText = page.locator("text=/tick \\d+/");
    const pausedText = await tickText.textContent();
    await page.waitForTimeout(1500);
    const stillPausedText = await tickText.textContent();
    expect(stillPausedText).toBe(pausedText);
  });

  test("resume button restarts tick counter", async ({ page }) => {
    // Pause first
    await page.locator("button", { hasText: /running/i }).click();
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Resume
    await page.locator("button", { hasText: /paused/i }).click();
    await expect(page.locator("button", { hasText: /running/i })).toBeVisible();

    // Verify ticks advancing
    const tickText = page.locator("text=/tick \\d+/");
    const afterResumeText = await tickText.textContent();
    const afterResumeTick = parseInt(afterResumeText!.match(/tick (\d+)/)![1], 10);
    await page.waitForTimeout(1500);
    const laterText = await tickText.textContent();
    const laterTick = parseInt(laterText!.match(/tick (\d+)/)![1], 10);
    expect(laterTick).toBeGreaterThan(afterResumeTick);
  });

  test("spacebar toggles pause", async ({ page }) => {
    // Press space to pause
    await page.keyboard.press("Space");
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Press space to resume
    await page.keyboard.press("Space");
    await expect(page.locator("button", { hasText: /running/i })).toBeVisible();
  });
});
```

**Step 2: Run the test**

Run: `cd e2e && npx playwright test tests/pause-resume.spec.ts`
Expected: PASS (3 tests)

**Step 3: Commit**

```bash
git add e2e/tests/pause-resume.spec.ts
git commit -m "test(e2e): pause/resume via button and spacebar

Tests pause stops tick counter, resume restarts it, and spacebar
toggles between states.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 9: E2E test — speed controls (VIO-149)

**Files:**
- Create: `e2e/tests/speed-controls.spec.ts`

**Step 1: Create the test file**

```typescript
import { test, expect } from "@playwright/test";

test.describe("Speed controls via keyboard presets", () => {
  test.beforeEach(async ({ page }) => {
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 10 }),
    });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("pressing 1 sets speed to 100 TPS", async ({ page }) => {
    await page.keyboard.press("Digit1");
    // Verify via API that speed was set
    const meta = await (await fetch("http://localhost:3002/api/v1/meta")).json();
    expect(meta.ticks_per_sec).toBe(100);
  });

  test("pressing 2 sets speed to 1K TPS", async ({ page }) => {
    await page.keyboard.press("Digit2");
    const meta = await (await fetch("http://localhost:3002/api/v1/meta")).json();
    expect(meta.ticks_per_sec).toBe(1000);
  });

  test("pressing 5 sets speed to max (0)", async ({ page }) => {
    await page.keyboard.press("Digit5");
    const meta = await (await fetch("http://localhost:3002/api/v1/meta")).json();
    expect(meta.ticks_per_sec).toBe(0);
  });
});
```

**Step 2: Run the test**

Run: `cd e2e && npx playwright test tests/speed-controls.spec.ts`
Expected: PASS (3 tests)

**Step 3: Commit**

```bash
git add e2e/tests/speed-controls.spec.ts
git commit -m "test(e2e): speed controls via keyboard presets

Verifies Digit1 (100 TPS), Digit2 (1K TPS), Digit5 (Max) keyboard
shortcuts set daemon speed correctly.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 10: E2E test — save game (VIO-148)

**Files:**
- Create: `e2e/tests/save-game.spec.ts`

**Step 1: Create the test file**

```typescript
import { test, expect } from "@playwright/test";

test.describe("Save game", () => {
  test.beforeEach(async ({ page }) => {
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("save button shows success feedback", async ({ page }) => {
    const saveButton = page.locator("button", { hasText: /^save$/i });
    await saveButton.click();
    // Should show "Saved" or success indicator
    await expect(page.locator("text=/saved/i")).toBeVisible({ timeout: 5_000 });
  });

  test("Cmd+S triggers save", async ({ page }) => {
    await page.keyboard.press("Meta+s");
    await expect(page.locator("text=/saved/i")).toBeVisible({ timeout: 5_000 });
  });
});
```

**Step 2: Run the test**

Run: `cd e2e && npx playwright test tests/save-game.spec.ts`
Expected: PASS (2 tests)

**Step 3: Commit**

```bash
git add e2e/tests/save-game.spec.ts
git commit -m "test(e2e): save game via button and Cmd+S

Verifies save button shows success feedback and Cmd+S keyboard
shortcut triggers the same behavior.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 11: E2E test — import command (VIO-147)

**Files:**
- Create: `e2e/tests/import-command.spec.ts`

This test is more complex — it interacts with the Economy panel. The exact selectors will depend on the Economy panel's current implementation. The implementer should:

**Step 1: Read the Economy panel component**

Read `ui_web/src/panels/EconomyPanel.tsx` to understand the form structure (select elements, input fields, button text).

**Step 2: Create the test file**

Write a test that:
1. Waits for app to load
2. Ensures Economy panel is visible (click nav button if needed)
3. Reads the initial balance from the status bar
4. Performs an import (select category, item, set quantity, click Import)
5. Waits for balance to decrease
6. Asserts the balance changed

The implementer must adapt selectors to the actual component markup. Use `page.locator("nav button", { hasText: "Economy" })` to open the panel, and inspect the form elements.

**Step 3: Run and iterate**

Run: `cd e2e && npx playwright test tests/import-command.spec.ts --headed`
Use headed mode to visually debug selector issues.

**Step 4: Commit**

```bash
git add e2e/tests/import-command.spec.ts
git commit -m "test(e2e): import command via Economy panel

Tests full import flow: open Economy panel, select item, set
quantity, import, verify balance decreases.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 12: CI integration (VIO-150)

**Files:**
- Create: `scripts/ci_e2e.sh`
- Modify: `.github/workflows/ci.yml`

**Step 1: Create `scripts/ci_e2e.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "=== E2E Tests ==="

# Build daemon
cargo build -p sim_daemon

# Install e2e deps
cd e2e
npm ci
npx playwright install chromium --with-deps

# Run tests
npx playwright test

echo "=== E2E Tests Complete ==="
```

Make it executable: `chmod +x scripts/ci_e2e.sh`

**Step 2: Add job to `.github/workflows/ci.yml`**

Add after the `bench-smoke` job:

```yaml
  e2e:
    name: E2E (Playwright)
    runs-on: ubuntu-latest
    needs: rust
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: |
            ui_web/package-lock.json
            e2e/package-lock.json
      - name: Install UI deps
        run: cd ui_web && npm ci --ignore-scripts
      - name: Run E2E tests
        run: ./scripts/ci_e2e.sh
      - uses: actions/upload-artifact@v4
        if: failure()
        with:
          name: e2e-test-results-${{ github.sha }}
          path: e2e/test-results/
          retention-days: 14
```

**Step 3: Commit**

```bash
git add scripts/ci_e2e.sh .github/workflows/ci.yml
git commit -m "ci: add E2E Playwright job to GitHub Actions

Runs after Rust build. Installs Chromium, runs all E2E tests.
Uploads test-results as artifact on failure.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 13: Update CLAUDE.md and docs

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add e2e commands to Common Commands section**

After the `cd mcp_advisor && npm start` line, add:

```markdown
cd e2e && npx playwright test                             # E2E tests
cd e2e && npx playwright test --headed                    # E2E tests (visible browser)
cd e2e && npx tsx screenshot-cli.ts / --output /tmp/s.png # Screenshot CLI
```

**Step 2: Add e2e to Architecture section**

Add after the `mcp_advisor` entry:

```markdown
- **e2e** — Playwright E2E tests + MCP screenshot server. Shared browser helpers in `lib/browser.ts`. Global setup spawns daemon (port 3002) + Vite (port 5174).
```

**Step 3: Add e2e CI script to commands**

After `./scripts/ci_bench_smoke.sh`, add:

```markdown
./scripts/ci_e2e.sh                                       # E2E Playwright tests
```

**Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add E2E testing commands and architecture

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```
