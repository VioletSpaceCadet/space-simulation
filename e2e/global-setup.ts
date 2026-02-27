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
  earlyExit?: () => Error | null,
): Promise<void> {
  for (let attempt = 0; attempt < retries; attempt++) {
    const exitError = earlyExit?.();
    if (exitError) throw exitError;
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

  // Spawn the compiled binary directly (not `cargo run`) so the PID we
  // record is the actual daemon process, not a cargo wrapper.
  const daemonBin = path.join(PROJECT_ROOT, "target", "debug", "sim_daemon");
  console.log("[e2e] Starting sim_daemon on port", DAEMON_PORT);
  const daemon = spawn(
    daemonBin,
    [
      "run",
      "--seed",
      "42",
      "--paused",
      "--port",
      String(DAEMON_PORT),
      "--cors-origin",
      `http://localhost:${VITE_PORT}`,
    ],
    { cwd: PROJECT_ROOT, stdio: "inherit", detached: false },
  );

  // Fail fast if the daemon exits unexpectedly during startup
  let daemonExitError: Error | null = null;
  daemon.on("exit", (code) => {
    if (code !== 0 && code !== null) {
      daemonExitError = new Error(`sim_daemon exited with code ${code}`);
    }
  });

  // Start Vite dev server
  console.log("[e2e] Starting Vite dev server on port", VITE_PORT);
  const vite = spawn(
    "npx",
    ["vite", "--port", String(VITE_PORT)],
    {
      cwd: path.join(PROJECT_ROOT, "ui_web"),
      stdio: "inherit",
      detached: false,
      env: {
        ...process.env,
        VITE_API_TARGET: `http://localhost:${DAEMON_PORT}`,
      },
    },
  );

  // Save PIDs for teardown
  if (!daemon.pid) throw new Error("Failed to spawn sim_daemon — no PID");
  if (!vite.pid) throw new Error("Failed to spawn Vite — no PID");
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
    () => daemonExitError,
  );

  console.log("[e2e] Waiting for Vite...");
  await waitForUrl(`http://localhost:${VITE_PORT}`, 30, 500);

  console.log("[e2e] Setup complete. Daemon and Vite are running.");
}
