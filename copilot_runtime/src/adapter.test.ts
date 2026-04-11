import { beforeEach, describe, expect, it, vi } from "vitest";
import { buildAdapterFromEnv, type AdapterDependencies } from "./adapter.js";

function buildStubDeps(
  overrides: Partial<AdapterDependencies> = {},
): { deps: AdapterDependencies; createProvider: ReturnType<typeof vi.fn> } {
  const stubChatModel = { specificationVersion: "v2", modelId: "stub" };
  const stubProvider = {
    chatModel: vi.fn((modelName: string) => ({ ...stubChatModel, modelId: modelName })),
  };
  const createProvider = vi.fn(() => stubProvider as never);

  return {
    deps: {
      getOpenRouterKey: vi.fn(() => "sk-or-test"),
      createProvider,
      ...overrides,
    },
    createProvider,
  };
}

describe("buildAdapterFromEnv", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("defaults to the OpenRouter provider with the Qwen 72B model", () => {
    const { deps, createProvider } = buildStubDeps();

    const config = buildAdapterFromEnv({}, deps);

    expect(config.provider).toBe("openrouter");
    expect(config.model).toBe("qwen/qwen-2.5-72b-instruct");
    expect(createProvider).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "openrouter",
        baseURL: "https://openrouter.ai/api/v1",
        apiKey: "sk-or-test",
      }),
    );
  });

  it("reads the Keychain-backed OpenRouter key via the injected helper", () => {
    const getOpenRouterKey = vi.fn(() => "sk-or-from-keychain");
    const { deps } = buildStubDeps({ getOpenRouterKey });

    buildAdapterFromEnv({ LLM_PROVIDER: "openrouter" }, deps);

    expect(getOpenRouterKey).toHaveBeenCalledTimes(1);
  });

  it("swaps to the Ollama provider when LLM_PROVIDER=ollama", () => {
    const getOpenRouterKey = vi.fn(() => "should-not-be-read");
    const { deps, createProvider } = buildStubDeps({ getOpenRouterKey });

    const config = buildAdapterFromEnv({ LLM_PROVIDER: "ollama" }, deps);

    expect(config.provider).toBe("ollama");
    expect(config.model).toBe("qwen2.5:14b-instruct");
    expect(createProvider).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "ollama",
        baseURL: "http://localhost:11434/v1",
        apiKey: "ollama",
      }),
    );
    // Decision 13: Ollama skips the Keychain call entirely.
    expect(getOpenRouterKey).not.toHaveBeenCalled();
  });

  it("allows overriding the model via LLM_MODEL env var", () => {
    const { deps } = buildStubDeps();

    const config = buildAdapterFromEnv(
      { LLM_PROVIDER: "openrouter", LLM_MODEL: "qwen/qwen3-30b-a3b-instruct" },
      deps,
    );

    expect(config.model).toBe("qwen/qwen3-30b-a3b-instruct");
  });

  it("ignores empty LLM_MODEL overrides", () => {
    const { deps } = buildStubDeps();

    const config = buildAdapterFromEnv(
      { LLM_PROVIDER: "ollama", LLM_MODEL: "   " },
      deps,
    );

    expect(config.model).toBe("qwen2.5:14b-instruct");
  });

  it("is case-insensitive about the provider name", () => {
    const { deps } = buildStubDeps();

    expect(buildAdapterFromEnv({ LLM_PROVIDER: "OpenRouter" }, deps).provider).toBe(
      "openrouter",
    );
    expect(buildAdapterFromEnv({ LLM_PROVIDER: "OLLAMA" }, deps).provider).toBe(
      "ollama",
    );
  });

  it("throws a helpful error for unknown providers", () => {
    const { deps } = buildStubDeps();

    expect(() =>
      buildAdapterFromEnv({ LLM_PROVIDER: "mystery-cloud" }, deps),
    ).toThrowError(/unknown LLM_PROVIDER.*openrouter.*ollama/);
  });
});
