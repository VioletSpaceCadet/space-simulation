/**
 * CopilotRuntime + BuiltInAgent wiring (v2 API).
 *
 * Everything here uses the `@copilotkit/runtime/v2` entrypoint. The v1
 * `copilotRuntimeNodeExpressEndpoint` helper ships a GraphQL/SSE surface
 * that a v2 `<CopilotKit>` frontend does not speak — mixing the two
 * produces the `Agent default not found` / `POST /api/copilotkit 404`
 * symptoms we hit on the first smoke test. The v2 `CopilotRuntime` +
 * `createCopilotExpressHandler` pair serves the canonical v2 wire format
 * that matches `@copilotkit/react-core/v2`'s expectations.
 *
 * `CopilotRuntime` is the v2 compatibility shim over `CopilotSseRuntime`;
 * new code could use `CopilotSseRuntime` directly, but the shim gives us a
 * single, stable public symbol.
 */

import { CopilotRuntime, BuiltInAgent } from "@copilotkit/runtime/v2";
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
