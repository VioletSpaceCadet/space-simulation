/**
 * Tests for the MCP client adapter (mcp.ts).
 *
 * These tests stub the transport and MCP client so they never spawn a child
 * process or touch the real mcp_advisor. The key behaviors under test:
 *
 * 1. Tool filtering — only the 5 read-only tools are exposed
 * 2. Lifecycle — close() propagates to the underlying client
 * 3. The adapter satisfies the MCPClientProvider interface
 */

import { describe, expect, it, vi } from "vitest";
import type { MCPClient, MCPClientConfig } from "@ai-sdk/mcp";
import type { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio";
import { createAdvisorMCPProvider, type MCPDependencies } from "./mcp.js";

// --- Stubs ---

/** Stub MCP tool definition matching the shape from listTools(). */
function stubToolDef(name: string) {
  return {
    name,
    description: `Stub: ${name}`,
    inputSchema: { type: "object" as const, properties: {} },
  };
}

/**
 * All 14 tools from mcp_advisor, matching the real server's tool list.
 * The test verifies only the allowed 5 make it through the filter.
 */
const ALL_TOOL_DEFS = [
  // Read-only (should pass filter)
  stubToolDef("get_metrics_digest"),
  stubToolDef("get_active_alerts"),
  stubToolDef("get_game_parameters"),
  stubToolDef("query_knowledge"),
  stubToolDef("get_strategy_config"),
  // Write tools (should be filtered out)
  stubToolDef("suggest_parameter_change"),
  stubToolDef("start_simulation"),
  stubToolDef("stop_simulation"),
  stubToolDef("set_speed"),
  stubToolDef("pause_simulation"),
  stubToolDef("resume_simulation"),
  stubToolDef("save_run_journal"),
  stubToolDef("update_playbook"),
  stubToolDef("suggest_strategy_change"),
];

const ALLOWED_TOOL_NAMES = new Set([
  "get_metrics_digest",
  "get_active_alerts",
  "get_game_parameters",
  "query_knowledge",
  "get_strategy_config",
]);

function createStubMCPClient(): MCPClient {
  const closeFn = vi.fn(async () => {});

  return {
    serverInfo: { name: "stub-advisor", version: "0.1.0" },
    listTools: vi.fn(async () => ({ tools: ALL_TOOL_DEFS })),
    toolsFromDefinitions: vi.fn((defs: { tools: typeof ALL_TOOL_DEFS }) => {
      // Return a record keyed by tool name with a stub execute function.
      const result: Record<string, { execute: () => Promise<unknown> }> = {};
      for (const tool of defs.tools) {
        result[tool.name] = { execute: async () => ({ content: [] }) };
      }
      return result;
    }),
    tools: vi.fn(async () => ({})),
    listResources: vi.fn(async () => ({ resources: [] })),
    readResource: vi.fn(async () => ({ contents: [] })),
    listResourceTemplates: vi.fn(async () => ({ resourceTemplates: [] })),
    experimental_listPrompts: vi.fn(async () => ({ prompts: [] })),
    experimental_getPrompt: vi.fn(async () => ({ messages: [] })),
    onElicitationRequest: vi.fn(),
    close: closeFn,
  } as unknown as MCPClient;
}

function createStubDeps(client?: MCPClient): MCPDependencies {
  const stubClient = client ?? createStubMCPClient();
  return {
    createClient: vi.fn(async (_config: MCPClientConfig) => stubClient),
    createTransport: vi.fn((_path: string) => ({}) as StdioClientTransport),
  };
}

// --- Tests ---

describe("createAdvisorMCPProvider", () => {
  it("creates a provider that satisfies MCPClientProvider", async () => {
    const deps = createStubDeps();
    const provider = await createAdvisorMCPProvider(deps);

    expect(provider).toHaveProperty("tools");
    expect(provider).toHaveProperty("close");
    expect(typeof provider.tools).toBe("function");
    expect(typeof provider.close).toBe("function");
  });

  it("filters tools to only the 5 read-only analytics tools", async () => {
    const deps = createStubDeps();
    const provider = await createAdvisorMCPProvider(deps);

    const tools = await provider.tools();
    const toolNames = Object.keys(tools);

    expect(toolNames).toHaveLength(5);
    for (const name of toolNames) {
      expect(ALLOWED_TOOL_NAMES.has(name)).toBe(true);
    }
  });

  it("excludes all write tools", async () => {
    const deps = createStubDeps();
    const provider = await createAdvisorMCPProvider(deps);

    const tools = await provider.tools();
    const toolNames = new Set(Object.keys(tools));

    const writeTools = [
      "suggest_parameter_change",
      "start_simulation",
      "stop_simulation",
      "set_speed",
      "pause_simulation",
      "resume_simulation",
      "save_run_journal",
      "update_playbook",
      "suggest_strategy_change",
    ];

    for (const name of writeTools) {
      expect(toolNames.has(name)).toBe(false);
    }
  });

  it("calls listTools then toolsFromDefinitions with filtered defs", async () => {
    const client = createStubMCPClient();
    const deps = createStubDeps(client);
    const provider = await createAdvisorMCPProvider(deps);

    await provider.tools();

    expect(client.listTools).toHaveBeenCalledOnce();
    expect(client.toolsFromDefinitions).toHaveBeenCalledOnce();

    // Verify the filtered definitions only contain allowed tools
    const filteredDefs = vi.mocked(client.toolsFromDefinitions).mock.calls[0]![0] as {
      tools: Array<{ name: string }>;
    };
    expect(filteredDefs.tools).toHaveLength(5);
    for (const tool of filteredDefs.tools) {
      expect(ALLOWED_TOOL_NAMES.has(tool.name)).toBe(true);
    }
  });

  it("close() delegates to the underlying MCP client", async () => {
    const client = createStubMCPClient();
    const deps = createStubDeps(client);
    const provider = await createAdvisorMCPProvider(deps);

    await provider.close();

    expect(client.close).toHaveBeenCalledOnce();
  });

  it("passes the correct config to createClient", async () => {
    const deps = createStubDeps();
    await createAdvisorMCPProvider(deps);

    expect(deps.createClient).toHaveBeenCalledOnce();
    const config = vi.mocked(deps.createClient).mock.calls[0]![0];
    expect(config.name).toBe("copilot-mcp-bridge");
    expect(config.version).toBe("0.1.0");
  });

  it("creates a transport with the resolved advisor path", async () => {
    const deps = createStubDeps();
    await createAdvisorMCPProvider(deps);

    expect(deps.createTransport).toHaveBeenCalledOnce();
    const advisorPath = vi.mocked(deps.createTransport).mock.calls[0]![0];
    expect(advisorPath).toMatch(/mcp_advisor\/dist\/index\.js$/);
  });
});
