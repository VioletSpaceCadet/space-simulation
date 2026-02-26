# MCP Daemon Auto-Start Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `start_simulation` and `stop_simulation` tools to the MCP advisor so Claude Code can launch the sim daemon without manual terminal work.

**Architecture:** The MCP server spawns `cargo run -p sim_daemon` as a child process, tracks the handle, polls for readiness, and auto-kills on exit. Single daemon at a time.

**Tech Stack:** TypeScript, node:child_process, existing MCP SDK

---

### Task 1: Add process management module state

**Files:**
- Modify: `mcp_advisor/src/index.ts`

**Step 1: Add child_process import and daemon state**

After the existing imports at the top of `index.ts`, add:

```typescript
import { spawn, type ChildProcess } from "node:child_process";
```

After the `CONTENT_DIR` constant (line 13), add:

```typescript
const PROJECT_ROOT = process.env["PROJECT_ROOT"] ?? path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "..",
);

let managedDaemon: ChildProcess | null = null;

function killManagedDaemon(): void {
  if (managedDaemon && !managedDaemon.killed) {
    managedDaemon.kill("SIGTERM");
    managedDaemon = null;
  }
}

process.on("exit", killManagedDaemon);
process.on("SIGINT", () => { killManagedDaemon(); process.exit(0); });
process.on("SIGTERM", () => { killManagedDaemon(); process.exit(0); });
```

**Step 2: Verify it compiles**

Run: `cd mcp_advisor && npx tsc`
Expected: No errors

**Step 3: Commit**

```bash
git add mcp_advisor/src/index.ts
git commit -m "feat(mcp_advisor): add daemon process management state"
```

---

### Task 2: Add `start_simulation` tool

**Files:**
- Modify: `mcp_advisor/src/index.ts`

**Step 1: Add the start_simulation tool**

After the `suggest_parameter_change` tool block (after line 189 closing `);`), add:

```typescript
// ---------- Tool 5: start_simulation ----------

async function waitForDaemon(retries = 30, intervalMs = 500): Promise<boolean> {
  for (let attempt = 0; attempt < retries; attempt++) {
    try {
      const response = await fetch(`${DAEMON_URL}/api/v1/meta`);
      if (response.ok) return true;
    } catch {
      // Daemon not ready yet
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  return false;
}

server.tool(
  "start_simulation",
  "Start a simulation daemon as a background process. Stops any previously started daemon first.",
  {
    seed: z.number().int().optional()
      .describe("RNG seed (default: random)"),
    max_ticks: z.number().int().optional()
      .describe("Stop after N ticks (default: unlimited)"),
  },
  async ({ seed, max_ticks }) => {
    killManagedDaemon();

    const actualSeed = seed ?? Math.floor(Math.random() * 2 ** 32);
    const args = [
      "run", "-p", "sim_daemon", "--",
      "run", "--seed", String(actualSeed),
    ];
    if (max_ticks !== undefined && max_ticks > 0) {
      args.push("--max-ticks", String(max_ticks));
    }

    const child = spawn("cargo", args, {
      cwd: PROJECT_ROOT,
      stdio: "ignore",
      detached: false,
    });

    child.on("error", (err) => {
      console.error(`[balance-advisor] daemon spawn error: ${err.message}`);
      managedDaemon = null;
    });

    child.on("exit", (code) => {
      console.error(`[balance-advisor] daemon exited with code ${code}`);
      if (managedDaemon === child) managedDaemon = null;
    });

    managedDaemon = child;

    const ready = await waitForDaemon();
    if (!ready) {
      killManagedDaemon();
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: "Daemon failed to start within 15 seconds. Check that cargo and sim_daemon build correctly.",
        }) }],
      };
    }

    return {
      content: [{ type: "text" as const, text: JSON.stringify({
        status: "started",
        seed: actualSeed,
        pid: child.pid,
      }) }],
    };
  },
);
```

**Step 2: Verify it compiles**

Run: `cd mcp_advisor && npx tsc`
Expected: No errors

**Step 3: Commit**

```bash
git add mcp_advisor/src/index.ts
git commit -m "feat(mcp_advisor): add start_simulation tool"
```

---

### Task 3: Add `stop_simulation` tool

**Files:**
- Modify: `mcp_advisor/src/index.ts`

**Step 1: Add the stop_simulation tool**

After the `start_simulation` tool block, add:

```typescript
// ---------- Tool 6: stop_simulation ----------

server.tool(
  "stop_simulation",
  "Stop a previously started simulation daemon",
  {},
  async () => {
    if (!managedDaemon || managedDaemon.killed) {
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "not_running",
          message: "No managed daemon is currently running.",
        }) }],
      };
    }

    const pid = managedDaemon.pid;
    killManagedDaemon();

    return {
      content: [{ type: "text" as const, text: JSON.stringify({
        status: "stopped",
        pid,
      }) }],
    };
  },
);
```

**Step 2: Verify it compiles**

Run: `cd mcp_advisor && npx tsc`
Expected: No errors

**Step 3: Commit**

```bash
git add mcp_advisor/src/index.ts
git commit -m "feat(mcp_advisor): add stop_simulation tool"
```

---

### Task 4: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update the Balance Advisor section**

In the Balance Advisor (MCP) section, add `start_simulation` and `stop_simulation` to the tool list and update the workflow:

Add to the tool list:
```markdown
- **start_simulation** — Start a sim daemon as a background process. Accepts optional `seed` and `max_ticks`. Stops any previous daemon first. Auto-kills on session end.
- **stop_simulation** — Stop a previously started sim daemon.
```

Replace the workflow paragraph with:
```markdown
**Workflow:** Use `start_simulation` to launch a daemon (or connect to one already running on port 3001). Wait for data to accumulate, then use `get_metrics_digest` to analyze trends. If something looks off, check `get_active_alerts` and `get_game_parameters` to understand why, then `suggest_parameter_change` to propose a fix. Use `stop_simulation` when done.
```

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with start/stop simulation tools"
```

---

### Task 5: Manual integration test

**Step 1: Rebuild the MCP advisor**

Run: `cd mcp_advisor && npx tsc`

**Step 2: Test via MCP tools (if available in session)**

Use the `start_simulation` MCP tool with seed 42. Verify it returns `{status: "started", seed: 42, pid: ...}`.

Then use `get_metrics_digest`. It may return `no_data` initially — wait a moment and retry. Should return a digest with trends and rates.

Then use `stop_simulation`. Verify it returns `{status: "stopped", pid: ...}`.

Then use `get_metrics_digest` again. Should return the connection error message.

**Step 3: Commit any fixes if needed**
