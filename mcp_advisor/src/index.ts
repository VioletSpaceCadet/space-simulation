import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import * as fs from "node:fs";
import * as path from "node:path";

const DAEMON_URL = process.env["DAEMON_URL"] ?? "http://localhost:3001";
const CONTENT_DIR = process.env["CONTENT_DIR"] ?? path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "..",
  "content",
);

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
    const response = await fetch(`${DAEMON_URL}/api/v1/advisor/digest`);
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
    const response = await fetch(`${DAEMON_URL}/api/v1/alerts`);
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
};

server.tool(
  "get_game_parameters",
  "Read game parameter files (constants, module_defs, techs, pricing)",
  {
    file: z.enum(["constants", "module_defs", "techs", "pricing", "all"])
      .describe("Which parameter file to read, or 'all' for everything"),
  },
  async ({ file }) => {
    if (file === "all") {
      const result: Record<string, unknown> = {};
      for (const [key, filename] of Object.entries(CONTENT_FILES)) {
        const filePath = path.join(CONTENT_DIR, filename);
        const raw = fs.readFileSync(filePath, "utf-8");
        result[key] = JSON.parse(raw);
      }
      return {
        content: [{ type: "text" as const, text: JSON.stringify(result, null, 2) }],
      };
    }

    const filename = CONTENT_FILES[file];
    const filePath = path.join(CONTENT_DIR, filename);
    const raw = fs.readFileSync(filePath, "utf-8");
    return { content: [{ type: "text" as const, text: raw }] };
  },
);

// ---------- Tool 4: suggest_parameter_change ----------

server.tool(
  "suggest_parameter_change",
  "Save a proposed parameter change for review",
  {
    parameter_path: z.string()
      .describe("Dotted path like constants.survey_scan_ticks"),
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
    const proposalsDir = path.join(CONTENT_DIR, "advisor_proposals");
    if (!fs.existsSync(proposalsDir)) {
      fs.mkdirSync(proposalsDir, { recursive: true });
    }

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

    fs.writeFileSync(filePath, JSON.stringify(proposal, null, 2) + "\n");

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
  },
);

// ---------- Start server ----------

const transport = new StdioServerTransport();
await server.connect(transport);
