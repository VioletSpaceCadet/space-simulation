/**
 * Language-model middleware: rewrite colliding text stream-part IDs.
 *
 * The bug
 * -------
 * `@ai-sdk/openai-compatible` (used for both OpenRouter and Ollama in this
 * sidecar) emits every text stream part with a hardcoded `id: "txt-0"`
 * regardless of turn. See:
 *   node_modules/@ai-sdk/openai-compatible/dist/index.js
 *   (`controller.enqueue({ type: "text-start", id: "txt-0" })`)
 *
 * CopilotKit's `BuiltInAgent.run()` forwards `part.id` verbatim into the
 * AG-UI `TEXT_MESSAGE_CHUNK.messageId` — its only guard is a literal
 * `providedId !== "0"` check, which `"txt-0"` slips through. Every
 * assistant turn ends up with the same messageId.
 *
 * On the client, `CopilotChatMessageView` calls `deduplicateMessages(...)`
 * which merges any two assistant messages with the same id — all turns
 * collapse into the first bubble. Each response is rendered in sequence
 * inside that bubble, producing the observed
 *   "Hello![response 1]Hello![response 2]Hello![response 3]" concatenation.
 * Reproduced on CopilotKit 1.55.3 + @ai-sdk/openai-compatible 2.0.41 via
 * Chrome automation; confirmed the same bug persists with all our frontend
 * hooks disabled.
 *
 * The fix
 * -------
 * Wrap the language model so every call to `doStream()` rewrites
 * `text-start`, `text-delta`, and `text-end` parts with a freshly-allocated
 * UUID per stream invocation. All deltas within a single turn share the
 * same UUID (so CopilotKit still accumulates the content into one message);
 * different turns get different UUIDs (so the dedup layer doesn't merge
 * them).
 *
 * Non-text parts (tool calls, reasoning, etc.) are passed through
 * unchanged — the bug only hits the `text-*` family.
 *
 * Scope note: this is a defensive wrapper at the adapter boundary, not a
 * patch to vendored CopilotKit source. When CopilotKit or the AI SDK fix
 * the upstream issue we can delete this file and the adapter call site
 * without touching any other code.
 */

import { randomUUID } from "node:crypto";

/**
 * Stream-part types where the AI SDK openai-compatible provider emits a
 * colliding hardcoded ID. Any part matching one of these types will have
 * its `id` rewritten to the per-stream UUID.
 */
const COLLIDING_PART_TYPES = new Set<string>([
  "text-start",
  "text-delta",
  "text-end",
]);

/**
 * Transforms a stream of `LanguageModelV2StreamPart` (or V3 — the shape is
 * identical for the fields we touch) by rewriting colliding text part IDs.
 */
function createIdRewriteTransform(freshId: string): TransformStream<unknown, unknown> {
  return new TransformStream({
    transform(chunk, controller) {
      if (
        chunk !== null &&
        typeof chunk === "object" &&
        "type" in chunk &&
        typeof (chunk as { type: unknown }).type === "string" &&
        COLLIDING_PART_TYPES.has((chunk as { type: string }).type) &&
        "id" in chunk &&
        typeof (chunk as { id: unknown }).id === "string"
      ) {
        controller.enqueue({ ...(chunk as Record<string, unknown>), id: freshId });
        return;
      }
      controller.enqueue(chunk);
    },
  });
}

/**
 * Wraps an AI SDK chat model so each `doStream()` invocation rewrites the
 * provider's hardcoded `"txt-0"` text-part IDs with a freshly-allocated
 * UUID. Works against both V2 and V3 language models since the `doStream`
 * return shape (`{ stream: ReadableStream<...> }`) and the text-part
 * structure (`{ type, id, ... }`) are identical across versions for the
 * fields this wrapper touches.
 *
 * The wrapper uses a `Proxy` to forward every property except `doStream`
 * untouched — so metadata, `doGenerate`, and provider options all behave
 * exactly like the underlying model.
 */
export function wrapChatModelWithUniqueTextIds<T extends object>(model: T): T {
  return new Proxy(model, {
    get(target, prop, receiver) {
      if (prop !== "doStream") {
        const value = Reflect.get(target, prop, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      }

      const originalDoStream = Reflect.get(target, "doStream", receiver) as (
        options: unknown,
      ) => Promise<{ stream: ReadableStream<unknown> } & Record<string, unknown>>;

      return async function wrappedDoStream(options: unknown) {
        const result = await originalDoStream.call(target, options);
        const freshId = randomUUID();
        return {
          ...result,
          stream: result.stream.pipeThrough(createIdRewriteTransform(freshId)),
        };
      };
    },
  });
}
