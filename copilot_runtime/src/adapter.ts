/**
 * Env-driven adapter factory.
 *
 * Plan decision 2: env-var provider swap (`LLM_PROVIDER=openrouter|ollama`) with
 * no abstraction layer beyond this factory. KISS. Both providers expose an
 * OpenAI-compatible chat API, which `@ai-sdk/openai-compatible` covers in one
 * call. `BuiltInAgent` consumes the returned `LanguageModelV2` directly.
 *
 * Phase A (Mb1, current): OpenRouter. Credentials come from macOS Keychain via
 *   credentials.ts (decision 13).
 * Phase B (Mb4, Mac Mini M4 Pro): Ollama local inference at
 *   `http://localhost:11434/v1`. The adapter passes the literal string
 *   `"ollama"` as the API key because the Ollama OpenAI-compatible endpoint
 *   ignores authorization headers — Ollama is localhost-only and skips the
 *   Keychain call entirely.
 */

import {
  createOpenAICompatible,
  type OpenAICompatibleProvider,
} from "@ai-sdk/openai-compatible";
import { getOpenRouterKey } from "./credentials.js";
import { wrapChatModelWithUniqueTextIds } from "./languageModelMiddleware.js";

/**
 * `ChatModel` is inferred from the provider return type so we don't need a
 * direct dependency on `@ai-sdk/provider`. BuiltInAgent accepts this shape as
 * the runtime equivalent of its `LanguageModel` type alias.
 */
export type ChatModel = ReturnType<OpenAICompatibleProvider["chatModel"]>;

export type LlmProviderName = "openrouter" | "ollama";

/**
 * The default model for each provider. These are sensible Phase-A/B picks per
 * the plan; overridable via `LLM_MODEL` env var for experiments without a
 * code change.
 */
const DEFAULT_MODELS: Record<LlmProviderName, string> = {
  openrouter: "qwen/qwen-2.5-72b-instruct",
  ollama: "qwen2.5:14b-instruct",
};

const OLLAMA_BASE_URL = "http://localhost:11434/v1";
const OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1";

export interface AdapterConfig {
  provider: LlmProviderName;
  model: string;
  chatModel: ChatModel;
}

function resolveProviderName(raw: string | undefined): LlmProviderName {
  const candidate = (raw ?? "openrouter").toLowerCase();
  if (candidate === "openrouter" || candidate === "ollama") { return candidate; }
  throw new Error(
    `copilot_runtime: unknown LLM_PROVIDER="${raw}". ` +
    "Expected \"openrouter\" or \"ollama\".",
  );
}

/**
 * Injectable dependencies so tests can substitute the key lookup and
 * `createOpenAICompatible` without touching the macOS Keychain or making real
 * HTTP calls.
 */
export interface AdapterDependencies {
  getOpenRouterKey: () => string;
  createProvider: typeof createOpenAICompatible;
}

const defaultDependencies: AdapterDependencies = {
  getOpenRouterKey,
  createProvider: createOpenAICompatible,
};

/**
 * Resolves the adapter configuration from environment variables. Called once
 * at startup in index.ts.
 *
 * Reads:
 *   LLM_PROVIDER — "openrouter" (default) or "ollama"
 *   LLM_MODEL    — override the default model name for the chosen provider
 */
export function buildAdapterFromEnv(
  env: NodeJS.ProcessEnv = process.env,
  deps: AdapterDependencies = defaultDependencies,
): AdapterConfig {
  const provider = resolveProviderName(env.LLM_PROVIDER);
  const model = env.LLM_MODEL?.trim() || DEFAULT_MODELS[provider];

  const openaiCompatible: OpenAICompatibleProvider =
    provider === "openrouter"
      ? deps.createProvider({
        name: "openrouter",
        baseURL: OPENROUTER_BASE_URL,
        apiKey: deps.getOpenRouterKey(),
      })
      : deps.createProvider({
        name: "ollama",
        baseURL: OLLAMA_BASE_URL,
        apiKey: "ollama",
      });

  // Wrap the raw chat model so every `doStream()` invocation allocates a
  // fresh UUID for text stream parts. Without this, CopilotKit's
  // `BuiltInAgent` forwards the openai-compatible provider's hardcoded
  // `"txt-0"` id as the AG-UI messageId, and the client merges all
  // assistant turns into a single bubble via deduplicateMessages. See
  // languageModelMiddleware.ts for the full diagnosis.
  const chatModel = wrapChatModelWithUniqueTextIds(openaiCompatible.chatModel(model));

  return {
    provider,
    model,
    chatModel,
  };
}
