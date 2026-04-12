---
title: "CopilotKit + AI SDK: txt-0 message ID collision causes merged chat bubbles"
category: integration-issues
date: 2026-04-12
tags:
  - copilotkit
  - ai-sdk
  - message-deduplication
  - language-model-middleware
  - ag-ui-protocol
  - openrouter
  - ollama
severity: high
components:
  - copilot_runtime
  - ui_web
  - "@copilotkit/runtime"
  - "@ai-sdk/openai-compatible"
  - "@copilotkit/react-core"
related_tickets:
  - VIO-676
  - VIO-677
  - VIO-680
  - VIO-681
---

## Symptom

In a CopilotKit v2 chat sidebar, all assistant responses concatenate into the
first assistant bubble. Multi-turn conversations render as:

```
user: hi
assistant: Hello!Hi there!Hello again!   ← all responses in ONE bubble
user: hey
user: how's it going
```

Instead of the expected:

```
user: hi
assistant: Hello!
user: hey
assistant: Hi there!
user: how's it going
assistant: Hello again!
```

The assistant bubble shows no line breaks between concatenated responses. User
messages render correctly as separate bubbles. The bug reproduces even with all
custom hooks (`useAgentContext`, `useFrontendTool`) disabled — it is NOT caused
by application code.

## Root Cause

Three components interact to produce the bug:

### 1. `@ai-sdk/openai-compatible` hardcodes stream-part IDs

In `@ai-sdk/openai-compatible@2.0.41`, the OpenAI-compatible chat model emits
every text stream part with a **hardcoded** `id: "txt-0"`:

```js
// node_modules/@ai-sdk/openai-compatible/dist/index.js:598
controller.enqueue({ type: "text-start", id: "txt-0" });
// :603
{ type: "text-delta", id: "txt-0", delta: chunk }
// :696
controller.enqueue({ type: "text-end", id: "txt-0" });
```

Every call to `doStream()` emits the same `"txt-0"` ID, regardless of turn.

### 2. CopilotKit BuiltInAgent forwards the ID verbatim

In `@copilotkit/runtime@1.55.3`, the `BuiltInAgent.run()` method receives the
stream part and uses its `id` as the AG-UI `TEXT_MESSAGE_CHUNK.messageId`:

```js
// node_modules/@copilotkit/runtime/dist/agent/index.cjs:582-584
case "text-start": {
  const providedId = "id" in part ? part.id : void 0;
  messageId = providedId && providedId !== "0" ? providedId : randomUUID();
  break;
}
```

The guard checks for the literal string `"0"`, but `"txt-0"` passes through.
Every turn's assistant message gets `messageId: "txt-0"`.

### 3. Client-side deduplication merges messages by ID

In `@copilotkit/react-core@1.55.3`, `CopilotChatMessageView` calls
`deduplicateMessages(messages)` which uses a Map keyed on `message.id`:

```js
// copilotkit-*.mjs (CopilotChatMessageView internals)
function deduplicateMessages(messages) {
  const acc = new Map();
  for (const message of messages) {
    const existing = acc.get(message.id);
    if (existing && message.role === "assistant" && existing.role === "assistant") {
      acc.set(message.id, { ...existing, ...message, content: message.content || existing.content });
    } else acc.set(message.id, message);
  }
  return [...acc.values()];
}
```

Since all assistant messages share `id: "txt-0"`, they merge into one entry.

## Investigation Steps

1. **Suspected v1/v2 API mixing** — switched all imports to `@copilotkit/runtime/v2`. Fixed the
   `Agent default not found` 404 error (see Related Pitfall #1) but did NOT fix the bubble
   concatenation.

2. **Tried single-route mode** — set `mode: "single-route"` on `createCopilotExpressHandler`.
   No effect on the concatenation bug.

3. **Tried `useDeferredValue`** on the snapshot readable to reduce context churn during
   conversations. No effect.

4. **Tried `throttleMs={250}`** on `<CopilotSidebar>` to batch message re-renders. No effect.

5. **Tried `useSingleEndpoint={false}`** to force multi-route transport. No effect.

6. **Disabled all custom hooks** (`useSnapshotReadable`, `useQueryActions`) in the
   `CopilotMissionBridge` component. **Bug persisted with a bare `<CopilotSidebar>`** — confirmed
   it was NOT caused by our hooks or context churn.

7. **Used Chrome MCP automation** to inspect the sidebar DOM. Found:
   - User messages: each had a unique UUID in `data-message-id`
   - Assistant messages: ALL had `data-message-id="txt-0"`

8. **Found `deduplicateMessages()`** in CopilotKit source with a development-mode warning:
   `"Merged N message(s) with duplicate IDs"`.

9. **Traced `txt-0`** back to `@ai-sdk/openai-compatible/dist/index.js:598` — hardcoded on every
   `text-start` event emission.

10. **Found the BuiltInAgent passthrough** at `agent/index.cjs:584` — the `!== "0"` guard does
    not catch `"txt-0"`.

## Solution

### Language model middleware (`copilot_runtime/src/languageModelMiddleware.ts`)

Wrap the `@ai-sdk/openai-compatible` chat model with a Proxy that intercepts
`doStream()` and rewrites all `text-start`, `text-delta`, `text-end` parts to
use a freshly-allocated UUID per stream invocation:

```typescript
import { randomUUID } from "node:crypto";

const COLLIDING_PART_TYPES = new Set(["text-start", "text-delta", "text-end"]);

function createIdRewriteTransform(freshId: string): TransformStream<unknown, unknown> {
  return new TransformStream({
    transform(chunk, controller) {
      if (
        chunk && typeof chunk === "object" &&
        "type" in chunk && COLLIDING_PART_TYPES.has((chunk as any).type) &&
        "id" in chunk
      ) {
        controller.enqueue({ ...(chunk as any), id: freshId });
        return;
      }
      controller.enqueue(chunk);
    },
  });
}

export function wrapChatModelWithUniqueTextIds<T extends object>(model: T): T {
  return new Proxy(model, {
    get(target, prop, receiver) {
      if (prop !== "doStream") {
        const value = Reflect.get(target, prop, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      }
      const originalDoStream = Reflect.get(target, "doStream", receiver) as Function;
      return async function wrappedDoStream(options: unknown) {
        const result = await originalDoStream.call(target, options);
        const freshId = randomUUID();
        return { ...result, stream: result.stream.pipeThrough(createIdRewriteTransform(freshId)) };
      };
    },
  });
}
```

### Wired into `copilot_runtime/src/adapter.ts`:

```typescript
import { wrapChatModelWithUniqueTextIds } from "./languageModelMiddleware.js";

const chatModel = wrapChatModelWithUniqueTextIds(openaiCompatible.chatModel(model));
```

### Why this approach

- **No vendored patches**: the fix is in our code, not in `node_modules`
- **Version-resilient**: wraps at the adapter boundary; if CopilotKit or the AI
  SDK fix the upstream issue, we delete one file and one call site
- **Works for both OpenRouter and Ollama**: both use `@ai-sdk/openai-compatible`
- **Non-text parts pass through unchanged**: only the `text-*` family is affected

## Verification

1. Chrome MCP DOM inspection confirmed each assistant message now has a unique
   `data-message-id` UUID
2. Multi-turn conversations render correctly with separate bubbles:
   ```
   user: hi
   assistant: Hello! How can I assist you with the game today?
   user: what is the treasury balance?
   assistant: The current treasury balance is $1,004,130,365. This data is from
              snapshot tick 105,563, which is the current (paused) state of the game.
   ```

## Related Pitfall #1: CopilotKit v1/v2 Import Mixing

Mixing v1 `CopilotRuntime` + `copilotRuntimeNodeExpressEndpoint` from
`@copilotkit/runtime` with v2 `BuiltInAgent` from `@copilotkit/runtime/v2`
produces `Agent default not found` / HTTP 404. The v1 runtime serves a
GraphQL/SSE surface that the v2 `<CopilotKit>` frontend does not speak.

**Fix:** Use ALL v2 imports on the server side:
```typescript
import { CopilotRuntime, BuiltInAgent } from "@copilotkit/runtime/v2";
import { createCopilotExpressHandler } from "@copilotkit/runtime/v2/express";
```

## Related Pitfall #2: CopilotKit v2 CSS Breaks vitest

CopilotKit v2's ESM build has side-effect CSS imports that vitest's Node loader
cannot parse. Tests that transitively import `@copilotkit/react-core/v2` fail
with `TypeError: Unknown file extension ".css"`.

**Fix:** Add to `ui_web/vite.config.ts`:
```typescript
test: {
  server: {
    deps: {
      inline: [/@copilotkit/],
    },
  },
}
```

This forces CopilotKit through Vite's transform pipeline, which handles CSS.

For pure-logic modules (selectors, action handlers), split into separate files
with no CopilotKit imports so vitest can test them without loading the runtime.
Example: `snapshotSelector.ts` (pure) vs `readables.ts` (hook wrapper).

## Prevention

- **When integrating a new AI SDK provider**: always verify that stream-part IDs
  are unique per invocation. Send two messages in the chat and check
  `data-message-id` attributes in the DOM — if they're the same, you've hit
  this bug.

- **When upgrading CopilotKit**: check if `BuiltInAgent.run()` now generates its
  own UUID unconditionally (the `providedId !== "0"` check might get broadened).
  If so, the middleware can be removed.

- **When mixing CopilotKit v1/v2 imports**: look for both `@copilotkit/runtime`
  (v1) and `@copilotkit/runtime/v2` in the same file. The runtime and endpoint
  helpers must all come from the same version.

- **When adding CopilotKit to a vitest-tested project**: add
  `server.deps.inline: [/@copilotkit/]` to the vitest config from the start.

## Affected Versions

- `@ai-sdk/openai-compatible`: 2.0.41 (hardcoded `txt-0`)
- `@copilotkit/runtime`: 1.55.3 (BuiltInAgent passthrough)
- `@copilotkit/react-core`: 1.55.3 (deduplicateMessages)
