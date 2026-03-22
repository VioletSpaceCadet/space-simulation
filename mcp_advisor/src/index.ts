import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import * as fs from "node:fs";
import * as fsPromises from "node:fs/promises";
import * as path from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import * as os from "node:os";
import * as crypto from "node:crypto";
import type { RunJournal } from "./types.js";

const DAEMON_URL = process.env["DAEMON_URL"] ?? "http://localhost:3001";
const CONTENT_DIR = process.env["CONTENT_DIR"] ?? path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "..",
  "content",
);

const PROJECT_ROOT = process.env["PROJECT_ROOT"] ?? path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "..",
);

function findCargo(): string {
  const homeCargo = path.join(os.homedir(), ".cargo", "bin", "cargo");
  if (fs.existsSync(homeCargo)) return homeCargo;
  return "cargo";
}

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

const server = new McpServer({
  name: "balance-advisor",
  version: "0.1.0",
});

// ---------- Tool 1: get_metrics_digest ----------

server.tool(
  "get_metrics_digest",
  "Fetch the latest metrics digest from the running simulation daemon",
  {},
  async () => {
    let response: Response;
    try {
      response = await fetch(`${DAEMON_URL}/api/v1/advisor/digest`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to connect to daemon at ${DAEMON_URL}: ${message}`,
        }) }],
      };
    }
    if (response.status === 204) {
      return {
        content: [
          {
            type: "text" as const,
            text: JSON.stringify({
              status: "no_data",
              message:
                "Simulation has no metrics history yet. Wait for the daemon to accumulate data.",
            }),
          },
        ],
      };
    }
    const body = await response.text();
    return { content: [{ type: "text" as const, text: body }] };
  },
);

// ---------- Tool 2: get_active_alerts ----------

server.tool(
  "get_active_alerts",
  "Fetch currently active alerts from the simulation daemon",
  {},
  async () => {
    let response: Response;
    try {
      response = await fetch(`${DAEMON_URL}/api/v1/alerts`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to connect to daemon at ${DAEMON_URL}: ${message}`,
        }) }],
      };
    }
    const body = await response.text();
    return { content: [{ type: "text" as const, text: body }] };
  },
);

// ---------- Tool 3: get_game_parameters ----------

const CONTENT_FILES: Record<string, string> = {
  constants: "constants.json",
  module_defs: "module_defs.json",
  techs: "techs.json",
  pricing: "pricing.json",
  solar_system: "solar_system.json",
};

server.tool(
  "get_game_parameters",
  "Read game parameter files (constants, module_defs, techs, pricing, solar_system)",
  {
    file: z.enum(["constants", "module_defs", "techs", "pricing", "solar_system", "all"])
      .describe("Which parameter file to read, or 'all' for everything"),
  },
  async ({ file }) => {
    try {
      if (file === "all") {
        const result: Record<string, unknown> = {};
        for (const [key, filename] of Object.entries(CONTENT_FILES)) {
          const filePath = path.join(CONTENT_DIR, filename);
          const raw = await fsPromises.readFile(filePath, "utf-8");
          result[key] = JSON.parse(raw);
        }
        return {
          content: [{ type: "text" as const, text: JSON.stringify(result, null, 2) }],
        };
      }

      const filename = CONTENT_FILES[file];
      const filePath = path.join(CONTENT_DIR, filename);
      const raw = await fsPromises.readFile(filePath, "utf-8");
      return { content: [{ type: "text" as const, text: raw }] };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to read content file: ${message}`,
        }) }],
      };
    }
  },
);

// ---------- Tool 4: suggest_parameter_change ----------

server.tool(
  "suggest_parameter_change",
  "Save a proposed parameter change for review",
  {
    parameter_path: z.string()
      .describe("Dotted path like constants.survey_scan_ticks or constants.ticks_per_au"),
    current_value: z.string()
      .describe("Current value as string"),
    proposed_value: z.string()
      .describe("Proposed new value as string"),
    rationale: z.string()
      .describe("Why this change is recommended"),
    expected_impact: z.string()
      .describe("What should improve with this change"),
  },
  async ({ parameter_path, current_value, proposed_value, rationale, expected_impact }) => {
    try {
      const proposalsDir = path.join(CONTENT_DIR, "advisor_proposals");
      await fsPromises.mkdir(proposalsDir, { recursive: true });

      const timestamp = Date.now();
      const filename = `proposal_${timestamp}.json`;
      const filePath = path.join(proposalsDir, filename);

      const proposal = {
        parameter_path,
        current_value,
        proposed_value,
        rationale,
        expected_impact,
        created_at: new Date(timestamp).toISOString(),
      };

      await fsPromises.writeFile(filePath, JSON.stringify(proposal, null, 2) + "\n");

      const relativePath = path.relative(
        path.resolve(CONTENT_DIR, ".."),
        filePath,
      );

      return {
        content: [
          {
            type: "text" as const,
            text: JSON.stringify({ status: "saved", path: relativePath }),
          },
        ],
      };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to save proposal: ${message}`,
        }) }],
      };
    }
  },
);

// ---------- Tool 5: start_simulation ----------

async function waitForDaemon(retries = 120, intervalMs = 500): Promise<boolean> {
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

    const cargoPath = findCargo();
    const actualSeed = seed ?? Math.floor(Math.random() * 2 ** 32);
    const args = [
      "run", "-p", "sim_daemon", "--",
      "run", "--seed", String(actualSeed),
    ];
    if (max_ticks !== undefined && max_ticks > 0) {
      args.push("--max-ticks", String(max_ticks));
    }

    const child = spawn(cargoPath, args, {
      cwd: PROJECT_ROOT,
      stdio: "ignore",
      detached: false,
    });

    managedDaemon = child;

    child.on("error", (err) => {
      console.error(`[balance-advisor] daemon spawn error: ${err.message}`);
      if (managedDaemon === child) managedDaemon = null;
    });

    child.on("exit", (code) => {
      console.error(`[balance-advisor] daemon exited with code ${code}`);
      if (managedDaemon === child) managedDaemon = null;
    });

    const ready = await waitForDaemon();
    if (!ready) {
      killManagedDaemon();
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: "Daemon failed to start within 60 seconds. Check that cargo and sim_daemon build correctly.",
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

// ---------- Tool 7: set_speed ----------

server.tool(
  "set_speed",
  "Set the simulation tick speed (ticks per second). Use higher values like 1000 for faster analysis.",
  {
    ticks_per_sec: z.number().min(0)
      .describe("Ticks per second (e.g. 10 for default, 1000 for fast analysis, 0 to pause)"),
  },
  async ({ ticks_per_sec }) => {
    let response: Response;
    try {
      response = await fetch(`${DAEMON_URL}/api/v1/speed`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ticks_per_sec }),
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to connect to daemon at ${DAEMON_URL}: ${message}`,
        }) }],
      };
    }
    const body = await response.text();
    return { content: [{ type: "text" as const, text: body }] };
  },
);

// ---------- Tool 8: pause_simulation ----------

server.tool(
  "pause_simulation",
  "Pause the running simulation. Use resume_simulation to continue.",
  {},
  async () => {
    let response: Response;
    try {
      response = await fetch(`${DAEMON_URL}/api/v1/pause`, { method: "POST" });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to connect to daemon at ${DAEMON_URL}: ${message}`,
        }) }],
      };
    }
    const body = await response.text();
    return { content: [{ type: "text" as const, text: body }] };
  },
);

// ---------- Tool 9: resume_simulation ----------

server.tool(
  "resume_simulation",
  "Resume a paused simulation.",
  {},
  async () => {
    let response: Response;
    try {
      response = await fetch(`${DAEMON_URL}/api/v1/resume`, { method: "POST" });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to connect to daemon at ${DAEMON_URL}: ${message}`,
        }) }],
      };
    }
    const body = await response.text();
    return { content: [{ type: "text" as const, text: body }] };
  },
);

// ---------- Tool 10: save_run_journal ----------

const ObservationSchema = z.object({
  metric: z.string().describe("Metric name matching MetricsSnapshot fields"),
  value: z.number().describe("Observed value"),
  trend: z.enum(["rising", "falling", "stable", "volatile"]).describe("Direction of change"),
  interpretation: z.string().describe("What this observation means"),
});

const BottleneckSchema = z.object({
  type: z.string().describe("Bottleneck category (e.g. ore_starvation)"),
  severity: z.enum(["low", "medium", "high", "critical"]).describe("Impact severity"),
  tick_range: z.tuple([z.number().int(), z.number().int()]).describe("[start_tick, end_tick]"),
  description: z.string().describe("Human-readable description"),
});

const JournalAlertSchema = z.object({
  alert_id: z.string().describe("Alert rule ID (e.g. ORE_STARVATION)"),
  severity: z.string().describe("Alert severity level"),
  first_seen_tick: z.number().int().describe("Tick when alert first fired"),
  resolved_tick: z.number().int().nullable().default(null).describe("Tick when alert cleared, or null"),
});

const ParameterChangeSchema = z.object({
  parameter_path: z.string().describe("Dotted path (e.g. constants.ticks_per_au)"),
  current_value: z.string().describe("Value before change"),
  proposed_value: z.string().describe("Value after change"),
  rationale: z.string().describe("Why the change was made"),
});

const BottleneckEventSchema = z.object({
  tick: z.number().int().describe("Tick when bottleneck state changed"),
  type: z.string().describe("Bottleneck category"),
  severity: z.enum(["low", "medium", "high", "critical"]).describe("Severity at this tick"),
});

server.tool(
  "save_run_journal",
  "Save a run journal entry after a simulation analysis session. Generates UUID and timestamp automatically.",
  {
    seed: z.number().int().describe("Simulation RNG seed used"),
    tick_range: z.tuple([z.number().int(), z.number().int()]).describe("[start_tick, end_tick] observed"),
    ticks_per_sec: z.number().optional().describe("Simulation speed used"),
    observations: z.array(ObservationSchema).describe("Structured observations"),
    bottlenecks: z.array(BottleneckSchema).describe("Bottlenecks detected"),
    alerts_seen: z.array(JournalAlertSchema).describe("Alerts that fired"),
    parameter_changes: z.array(ParameterChangeSchema)
      .describe("Parameter changes proposed or applied"),
    strategy_notes: z.array(z.string()).describe("Free-form learnings"),
    tags: z.array(z.string()).describe("Categorization tags"),
    final_score: z.number().optional().describe("Composite metric at run end"),
    collapse_tick: z.number().int().nullable().optional().describe("Tick of collapse, or null"),
    bottleneck_timeline: z.array(BottleneckEventSchema).optional()
      .describe("Time-series bottleneck state changes"),
    autopilot_config_hash: z.string().optional().describe("Hash of autopilot config"),
    parquet_path: z.string().optional().describe("Path to associated Parquet file"),
  },
  async (params) => {
    try {
      const journalsDir = path.join(CONTENT_DIR, "knowledge", "journals");
      await fsPromises.mkdir(journalsDir, { recursive: true });

      const id = crypto.randomUUID();
      const timestamp = new Date().toISOString();
      const fileTimestamp = timestamp.replace(/:/g, "-").replace(/\.\d+Z$/, "Z");
      const idFragment = id.split("-")[0];
      const filename = `${fileTimestamp}_${params.seed}_${idFragment}.json`;
      const filePath = path.join(journalsDir, filename);

      const journal: RunJournal = {
        id,
        timestamp,
        seed: params.seed,
        tick_range: params.tick_range,
        ...(params.ticks_per_sec !== undefined && { ticks_per_sec: params.ticks_per_sec }),
        observations: params.observations,
        bottlenecks: params.bottlenecks,
        alerts_seen: params.alerts_seen,
        parameter_changes: params.parameter_changes,
        strategy_notes: params.strategy_notes,
        tags: params.tags,
        ...(params.final_score !== undefined && { final_score: params.final_score }),
        ...(params.collapse_tick !== undefined && { collapse_tick: params.collapse_tick }),
        ...(params.bottleneck_timeline !== undefined && { bottleneck_timeline: params.bottleneck_timeline }),
        ...(params.autopilot_config_hash !== undefined && { autopilot_config_hash: params.autopilot_config_hash }),
        ...(params.parquet_path !== undefined && { parquet_path: params.parquet_path }),
      };

      await fsPromises.writeFile(filePath, JSON.stringify(journal, null, 2) + "\n");

      const relativePath = path.relative(
        path.resolve(CONTENT_DIR, ".."),
        filePath,
      );

      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "saved",
          id,
          path: relativePath,
          timestamp,
        }) }],
      };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to save journal: ${message}`,
        }) }],
      };
    }
  },
);

// ---------- Tool 11: update_playbook ----------

const PLAYBOOK_PATH = path.join(CONTENT_DIR, "knowledge", "playbook.md");

/**
 * Find a section in a markdown document by matching header text.
 * Supports nested headers via ">" separator (e.g. "Bottleneck Resolutions > Ore Supply").
 * Returns the start index (end of header line) and end index (start of next same-or-higher-level header).
 */
function findSection(
  lines: string[],
  sectionPath: string,
): { startLine: number; endLine: number; level: number } | null {
  const parts = sectionPath.split(">").map((s) => s.trim().toLowerCase());
  const target = parts[parts.length - 1];

  // Build a heading index for parent validation
  const headings: { level: number; text: string; line: number }[] = [];
  for (let index = 0; index < lines.length; index++) {
    const match = lines[index].match(/^(#{1,6})\s+(.+)$/);
    if (match) {
      headings.push({ level: match[1].length, text: match[2].trim().toLowerCase(), line: index });
    }
  }

  for (let hi = 0; hi < headings.length; hi++) {
    const heading = headings[hi];
    if (heading.text !== target) continue;

    // Validate parent path: walk up the heading hierarchy
    if (parts.length > 1) {
      let valid = true;
      let checkIdx = hi;
      for (let pi = parts.length - 2; pi >= 0; pi--) {
        // Find the nearest ancestor heading with a lower level
        let found = false;
        for (let ai = checkIdx - 1; ai >= 0; ai--) {
          if (headings[ai].level < headings[checkIdx].level) {
            if (headings[ai].text !== parts[pi]) {
              valid = false;
            }
            checkIdx = ai;
            found = true;
            break;
          }
        }
        if (!found) { valid = false; }
        if (!valid) break;
      }
      if (!valid) continue;
    }

    // Found a match — compute section end
    const index = heading.line;
    let endLine = lines.length;
    for (let j = hi + 1; j < headings.length; j++) {
      if (headings[j].level <= heading.level) {
        endLine = headings[j].line;
        break;
      }
    }
    return { startLine: index, endLine, level: heading.level };
  }
  return null;
}

server.tool(
  "update_playbook",
  "Append to or replace a section of the strategy playbook (content/knowledge/playbook.md)",
  {
    section: z.string()
      .describe("Section header to update (e.g. 'Bottleneck Resolutions > Ore Supply')"),
    content: z.string()
      .describe("Markdown content to append or replace"),
    mode: z.enum(["append", "replace"]).default("append")
      .describe("'append' adds to end of section, 'replace' replaces section content"),
  },
  async ({ section, content: newContent, mode }) => {
    try {
      let fileContent: string;
      try {
        fileContent = await fsPromises.readFile(PLAYBOOK_PATH, "utf-8");
      } catch {
        return {
          content: [{ type: "text" as const, text: JSON.stringify({
            status: "error",
            message: "Playbook not found at content/knowledge/playbook.md",
          }) }],
        };
      }

      const lines = fileContent.split("\n");
      const found = findSection(lines, section);

      if (!found) {
        return {
          content: [{ type: "text" as const, text: JSON.stringify({
            status: "error",
            message: `Section not found: "${section}". Available top-level sections: ${
              lines
                .filter((l) => /^##\s+/.test(l))
                .map((l) => l.replace(/^##\s+/, ""))
                .join(", ")
            }`,
          }) }],
        };
      }

      const contentLines = newContent.split("\n");
      let updatedLines: string[];

      if (mode === "replace") {
        // Keep the header, replace everything until next section
        updatedLines = [
          ...lines.slice(0, found.startLine + 1),
          "",
          ...contentLines,
          "",
          ...lines.slice(found.endLine),
        ];
      } else {
        // Append before the next section boundary, trimming trailing blanks to avoid accumulation
        let insertAt = found.endLine;
        while (insertAt > found.startLine + 1 && lines[insertAt - 1].trim() === "") {
          insertAt--;
        }
        updatedLines = [
          ...lines.slice(0, insertAt),
          ...contentLines,
          "",
          ...lines.slice(found.endLine),
        ];
      }

      await fsPromises.writeFile(PLAYBOOK_PATH, updatedLines.join("\n"));

      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "updated",
          section,
          mode,
          lines_added: contentLines.length,
        }) }],
      };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        content: [{ type: "text" as const, text: JSON.stringify({
          status: "error",
          message: `Failed to update playbook: ${message}`,
        }) }],
      };
    }
  },
);

// ---------- Startup validation ----------

function validateContentDir(): void {
  if (!fs.existsSync(CONTENT_DIR)) {
    console.error(`[balance-advisor] WARNING: CONTENT_DIR does not exist: ${CONTENT_DIR}`);
    return;
  }
  for (const [key, filename] of Object.entries(CONTENT_FILES)) {
    const filePath = path.join(CONTENT_DIR, filename);
    if (!fs.existsSync(filePath)) {
      console.error(`[balance-advisor] WARNING: missing content file "${key}": ${filePath}`);
    }
  }
}

validateContentDir();

// ---------- Start server ----------

const transport = new StdioServerTransport();
await server.connect(transport);
