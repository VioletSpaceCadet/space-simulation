/**
 * Smoke tests for `buildRuntime`.
 *
 * These exist because the glue between CopilotKit v2 `CopilotRuntime` and v2
 * `BuiltInAgent` is load-bearing and not otherwise exercised by a unit test —
 * the adapter tests stub `createOpenAICompatible` so they never touch the
 * runtime itself. A test that simply constructs the runtime and asserts the
 * agent lands in `runtime.agents` catches the class of bug where we
 * accidentally mix v1/v2 imports (v1 CopilotRuntime wrapping a v2
 * BuiltInAgent silently produces `Agent default not found` when the
 * frontend posts to the wrong endpoint shape).
 */

import { describe, expect, it } from "vitest";
import { CopilotRuntime } from "@copilotkit/runtime/v2";
import type { ChatModel } from "./adapter.js";
import { buildRuntime } from "./runtime.js";

// Minimal stub for `LanguageModelV2`. The runtime never invokes the model at
// construction time; it only stores the reference on the agent. A plausible
// shape satisfies the structural type check.
const stubChatModel = {
  specificationVersion: "v2",
  provider: "stub",
  modelId: "stub-model",
  supportedUrls: {},
  doGenerate: () => { throw new Error("stub"); },
  doStream: () => { throw new Error("stub"); },
} as unknown as ChatModel;

describe("buildRuntime", () => {
  it("constructs a v2 CopilotRuntime with a `default` agent registered", async () => {
    const runtime = buildRuntime({
      adapter: {
        provider: "openrouter",
        model: "qwen/qwen-2.5-72b-instruct",
        chatModel: stubChatModel,
      },
    });

    expect(runtime).toBeInstanceOf(CopilotRuntime);

    // `runtime.agents` may be a Record or a Promise<Record> depending on the
    // v2 lazy-loading contract. Normalize and assert `default` is registered.
    const agents = await Promise.resolve(runtime.agents);
    expect(agents).toBeDefined();
    expect(Object.keys(agents as Record<string, unknown>)).toContain("default");
  });

  it("constructs successfully regardless of the provider variant", () => {
    const openrouter = buildRuntime({
      adapter: {
        provider: "openrouter",
        model: "qwen/qwen-2.5-72b-instruct",
        chatModel: stubChatModel,
      },
    });
    const ollama = buildRuntime({
      adapter: {
        provider: "ollama",
        model: "qwen2.5:14b-instruct",
        chatModel: stubChatModel,
      },
    });

    expect(openrouter).toBeInstanceOf(CopilotRuntime);
    expect(ollama).toBeInstanceOf(CopilotRuntime);
  });

  it("accepts MCP clients when provided", async () => {
    const stubProvider = {
      tools: async () => ({}),
    };

    const runtime = buildRuntime({
      adapter: {
        provider: "openrouter",
        model: "qwen/qwen-2.5-72b-instruct",
        chatModel: stubChatModel,
      },
      mcpClients: [stubProvider],
    });

    expect(runtime).toBeInstanceOf(CopilotRuntime);

    const agents = await Promise.resolve(runtime.agents);
    expect(Object.keys(agents as Record<string, unknown>)).toContain("default");
  });

  it("works without MCP clients", () => {
    const runtime = buildRuntime({
      adapter: {
        provider: "openrouter",
        model: "qwen/qwen-2.5-72b-instruct",
        chatModel: stubChatModel,
      },
    });

    expect(runtime).toBeInstanceOf(CopilotRuntime);
  });
});
