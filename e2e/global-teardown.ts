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
