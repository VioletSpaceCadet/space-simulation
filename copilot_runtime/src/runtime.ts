/**
 * CopilotRuntime + BuiltInAgent wiring.
 *
 * Separated from index.ts so tests can build a runtime with a stub adapter
 * without standing up the whole Express server.
 *
 * Note on system prompt: Mb1 ships a minimal one ("you are a space sim
 * co-pilot in development — answer briefly from the dummy snapshot"). Mb2
 * replaces this with a fuller prompt that references the real hierarchical
 * readable.
 */

import { CopilotRuntime } from "@copilotkit/runtime";
import { BuiltInAgent } from "@copilotkit/runtime/v2";
import type { AdapterConfig } from "./adapter.js";

const SYSTEM_PROMPT =
  "You are the mission co-pilot for a space industry simulation game. " +
  "This is the Mb1 smoke-test environment: a dummy snapshot provides a " +
  "single fake tick number. Answer briefly, cite the tick from the snapshot " +
  "when asked, and do not invent new game state. More tools arrive in Mb2.";

// Plan decision 2: temperature 0.2. The sim is deterministic; we want the LLM
// to be as close to deterministic as sampling allows.
const LLM_TEMPERATURE = 0.2;

export function buildRuntime(adapter: AdapterConfig): CopilotRuntime {
  const agent = new BuiltInAgent({
    model: adapter.chatModel,
    prompt: SYSTEM_PROMPT,
    temperature: LLM_TEMPERATURE,
  });

  return new CopilotRuntime({
    agents: { default: agent },
  });
}
