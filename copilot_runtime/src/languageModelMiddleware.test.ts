import { describe, expect, it } from "vitest";
import { wrapChatModelWithUniqueTextIds } from "./languageModelMiddleware.js";

/** Collect all chunks from a ReadableStream into an array. */
async function collectStream<T>(stream: ReadableStream<T>): Promise<T[]> {
  const reader = stream.getReader();
  const chunks: T[] = [];
  for (;;) {
    const { done, value } = await reader.read();
    if (done) { break; }
    chunks.push(value);
  }
  return chunks;
}

/** Create a ReadableStream from an array of chunks. */
function streamFrom<T>(chunks: T[]): ReadableStream<T> {
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) { controller.enqueue(chunk); }
      controller.close();
    },
  });
}

interface FakeModel {
  modelId: string;
  doStream: (options: unknown) => Promise<{ stream: ReadableStream<unknown> }>;
  doGenerate: () => string;
}

function makeFakeModel(chunks: unknown[]): FakeModel {
  return {
    modelId: "test-model",
    doStream: async () => ({ stream: streamFrom(chunks) }),
    doGenerate: () => "not-proxied",
  };
}

describe("wrapChatModelWithUniqueTextIds", () => {
  it("rewrites text-start, text-delta, and text-end IDs to a fresh UUID", async () => {
    const chunks = [
      { type: "text-start", id: "txt-0", value: "" },
      { type: "text-delta", id: "txt-0", value: "Hello" },
      { type: "text-delta", id: "txt-0", value: " world" },
      { type: "text-end", id: "txt-0", value: "" },
    ];

    const wrapped = wrapChatModelWithUniqueTextIds(makeFakeModel(chunks));
    const result = await wrapped.doStream({});
    const output = await collectStream(result.stream) as Array<{ type: string; id: string; value: string }>;

    expect(output).toHaveLength(4);
    // All text parts share the same fresh UUID (not "txt-0")
    const ids = new Set(output.map((c) => c.id));
    expect(ids.size).toBe(1);
    const freshId = output[0]!.id;
    expect(freshId).not.toBe("txt-0");
    // UUID format check (8-4-4-4-12 hex)
    expect(freshId).toMatch(/^[\da-f]{8}-[\da-f]{4}-[\da-f]{4}-[\da-f]{4}-[\da-f]{12}$/);
    // Content preserved
    expect(output.map((c) => c.value)).toEqual(["", "Hello", " world", ""]);
  });

  it("passes non-text parts through unchanged", async () => {
    const chunks = [
      { type: "tool-call-start", id: "tool-1", toolName: "query" },
      { type: "text-delta", id: "txt-0", value: "hi" },
      { type: "tool-call-end", id: "tool-1" },
    ];

    const wrapped = wrapChatModelWithUniqueTextIds(makeFakeModel(chunks));
    const result = await wrapped.doStream({});
    const output = await collectStream(result.stream) as Array<{ type: string; id: string }>;

    expect(output).toHaveLength(3);
    // Tool parts keep original IDs
    expect(output[0]!.id).toBe("tool-1");
    expect(output[2]!.id).toBe("tool-1");
    // Text part gets rewritten
    expect(output[1]!.id).not.toBe("txt-0");
  });

  it("generates different UUIDs per doStream invocation", async () => {
    const chunks = [{ type: "text-start", id: "txt-0", value: "" }];
    const wrapped = wrapChatModelWithUniqueTextIds(makeFakeModel(chunks));

    const result1 = await wrapped.doStream({});
    const output1 = await collectStream(result1.stream) as Array<{ type: string; id: string }>;

    const result2 = await wrapped.doStream({});
    const output2 = await collectStream(result2.stream) as Array<{ type: string; id: string }>;

    expect(output1[0]!.id).not.toBe(output2[0]!.id);
  });

  it("forwards non-doStream properties and methods unchanged", () => {
    const model = makeFakeModel([]);
    const wrapped = wrapChatModelWithUniqueTextIds(model);

    expect(wrapped.modelId).toBe("test-model");
    expect(wrapped.doGenerate()).toBe("not-proxied");
  });

  it("handles non-object chunks gracefully (passthrough)", async () => {
    const chunks = [null, "plain-string", 42, { type: "text-delta", id: "txt-0", value: "ok" }];
    const wrapped = wrapChatModelWithUniqueTextIds(makeFakeModel(chunks));
    const result = await wrapped.doStream({});
    const output = await collectStream(result.stream);

    expect(output).toHaveLength(4);
    expect(output[0]).toBeNull();
    expect(output[1]).toBe("plain-string");
    expect(output[2]).toBe(42);
    expect((output[3] as { id: string }).id).not.toBe("txt-0");
  });
});
