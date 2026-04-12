/**
 * MCP client adapter for the balance-advisor MCP server.
 *
 * Spawns `mcp_advisor` as a stdio child process via `StdioClientTransport`
 * and exposes a filtered subset of its tools to CopilotKit's BuiltInAgent
 * via the `MCPClientProvider` interface.
 *
 * Only read-only analytics tools are exposed to the in-game copilot:
 *   - get_metrics_digest — daemon analytics (trends, rates, bottlenecks)
 *   - get_active_alerts  — active alert list from the daemon
 *   - get_game_parameters — content files (constants, module_defs, techs, etc.)
 *   - query_knowledge    — search past run journals and strategy playbook
 *   - get_strategy_config — current autopilot strategy settings
 *
 * Write tools (sim lifecycle, parameter proposals, knowledge mutations) are
 * excluded — sim control is already covered by the approval-card actions,
 * and admin tools aren't player-facing.
 *
 * This adapter is the single file that touches the MCP client API. If
 * CopilotKit or `@ai-sdk/mcp` change their interfaces upstream, breakage
 * is contained here (plan decision 4).
 */

import path from "node:path";
import { fileURLToPath } from "node:url";
import { createMCPClient, type MCPClient } from "@ai-sdk/mcp";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio";
import type { MCPClientProvider } from "@copilotkit/runtime/v2";

/**
 * Read-only tools safe for the in-game copilot. Every name listed here is
 * forwarded to the LLM; everything else is filtered out.
 */
const ALLOWED_TOOLS = new Set([
  "get_metrics_digest",
  "get_active_alerts",
  "get_game_parameters",
  "query_knowledge",
  "get_strategy_config",
]);

/**
 * Extended MCPClientProvider with a `close()` method for lifecycle management.
 * CopilotKit calls `tools()` on each agent run; we own the child process
 * lifetime so callers must `close()` on shutdown.
 */
export interface AdvisorMCPProvider extends MCPClientProvider {
  close(): Promise<void>;
}

/** Injected dependencies for testing. */
export interface MCPDependencies {
  createClient: typeof createMCPClient;
  createTransport: (advisorPath: string) => StdioClientTransport;
}

function defaultCreateTransport(advisorPath: string): StdioClientTransport {
  return new StdioClientTransport({
    command: "node",
    args: [advisorPath],
    stderr: "pipe",
  });
}

const defaultDeps: MCPDependencies = {
  createClient: createMCPClient,
  createTransport: defaultCreateTransport,
};

/**
 * Resolve the path to mcp_advisor's built entry point.
 * Relative to this file: `../../mcp_advisor/dist/index.js`.
 */
function resolveAdvisorPath(): string {
  const thisDir = path.dirname(fileURLToPath(import.meta.url));
  return path.resolve(thisDir, "../../mcp_advisor/dist/index.js");
}

/**
 * Create an MCP client connected to the balance-advisor stdio server.
 *
 * Returns an `MCPClientProvider` (for BuiltInAgent.mcpClients) that only
 * exposes the read-only tool subset, plus a `close()` method.
 *
 * @throws if mcp_advisor fails to start or the MCP handshake fails.
 */
export async function createAdvisorMCPProvider(
  deps: MCPDependencies = defaultDeps,
): Promise<AdvisorMCPProvider> {
  const advisorPath = resolveAdvisorPath();
  const transport = deps.createTransport(advisorPath);

  const client: MCPClient = await deps.createClient({
    transport,
    name: "copilot-mcp-bridge",
    version: "0.1.0",
  });

  return {
    async tools() {
      // Fetch the full tool list, filter to allowed names, then convert
      // the filtered definitions into AI SDK tools that CopilotKit can call.
      const allDefs = await client.listTools();
      const filtered = {
        ...allDefs,
        tools: allDefs.tools.filter((t) => ALLOWED_TOOLS.has(t.name)),
      };
      return client.toolsFromDefinitions(filtered);
    },
    close: () => client.close(),
  };
}
