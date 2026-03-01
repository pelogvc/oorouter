import { describe, expect, test } from "vitest"
import {
  createOpenAIStreamContext,
  createOpenAIStreamTransformer,
  collectOpenAIResponse,
} from "../../src/providers/codex/openai-streaming"

function makeSSE(...events: string[]): string {
  return events.map((e) => `data: ${e}\n\n`).join("")
}

async function collectStream(sseData: string, includeUsage = false): Promise<string[]> {
  const encoder = new TextEncoder()
  const ctx = createOpenAIStreamContext("test-model", includeUsage)
  const transformer = createOpenAIStreamTransformer(ctx)

  const input = new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(sseData))
      controller.close()
    },
  })

  const reader = input
    .pipeThrough(new TextDecoderStream())
    .pipeThrough(transformer)
    .getReader()

  const chunks: string[] = []
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    chunks.push(value)
  }
  return chunks
}

function parseChunk(raw: string): Record<string, unknown> {
  const line = raw.trim()
  if (line.startsWith("data: {")) {
    return JSON.parse(line.replace("data: ", "")) as Record<string, unknown>
  }
  return { raw: line }
}

describe("createOpenAIStreamContext", () => {
  test("generates unique completionId and systemFingerprint", () => {
    const ctx = createOpenAIStreamContext("model-a", false)
    expect(ctx.completionId).toMatch(/^chatcmpl-/)
    expect(ctx.systemFingerprint).toMatch(/^fp_/)
    expect(ctx.model).toBe("model-a")
    expect(ctx.includeUsage).toBe(false)
  })

  test("creates different ids on each call", () => {
    const a = createOpenAIStreamContext("m", true)
    const b = createOpenAIStreamContext("m", true)
    expect(a.completionId).not.toBe(b.completionId)
    expect(a.systemFingerprint).not.toBe(b.systemFingerprint)
  })
})

describe("createOpenAIStreamTransformer - text streaming", () => {
  test("sends role chunk then content then finish and [DONE]", async () => {
    const sse = makeSSE(
      '{"type":"response.output_text.delta","delta":"Hello"}',
      '{"type":"response.output_text.delta","delta":" world"}',
      '{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}'
    )

    const chunks = await collectStream(sse)
    const dataChunks = chunks.filter((c) => c.startsWith("data: {"))

    // role + 2 content + finish = 4
    expect(dataChunks.length).toBe(4)

    const role = parseChunk(dataChunks[0])
    const choices0 = (role.choices as { delta: { role: string } }[])[0]
    expect(choices0.delta.role).toBe("assistant")

    const content1 = parseChunk(dataChunks[1])
    const c1 = (content1.choices as { delta: { content: string } }[])[0]
    expect(c1.delta.content).toBe("Hello")

    const content2 = parseChunk(dataChunks[2])
    const c2 = (content2.choices as { delta: { content: string } }[])[0]
    expect(c2.delta.content).toBe(" world")

    const finish = parseChunk(dataChunks[3])
    const f = (finish.choices as { finish_reason: string }[])[0]
    expect(f.finish_reason).toBe("stop")

    // [DONE] at the end
    expect(chunks[chunks.length - 1]).toContain("[DONE]")
  })

  test("all chunks share same id, created, model, system_fingerprint", async () => {
    const sse = makeSSE(
      '{"type":"response.output_text.delta","delta":"x"}',
      '{"type":"response.completed","response":{"id":"r1"}}'
    )

    const chunks = await collectStream(sse)
    const parsed = chunks.filter((c) => c.startsWith("data: {")).map(parseChunk)

    const firstId = parsed[0].id
    const firstCreated = parsed[0].created
    const firstFp = parsed[0].system_fingerprint

    for (const p of parsed) {
      expect(p.id).toBe(firstId)
      expect(p.created).toBe(firstCreated)
      expect(p.system_fingerprint).toBe(firstFp)
      expect(p.object).toBe("chat.completion.chunk")
      expect(p.model).toBe("test-model")
    }
  })

  test("include_usage=true includes usage on finish chunk", async () => {
    const sse = makeSSE(
      '{"type":"response.output_text.delta","delta":"hi"}',
      '{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}'
    )

    const chunks = await collectStream(sse, true)
    const dataChunks = chunks.filter((c) => c.startsWith("data: {")).map(parseChunk)

    // Content chunks have usage: null
    const contentChunk = dataChunks[1] // first content
    expect(contentChunk.usage).toBeNull()

    // Finish chunk has actual usage
    const finishChunk = dataChunks[dataChunks.length - 1]
    const usage = finishChunk.usage as { prompt_tokens: number; completion_tokens: number }
    expect(usage.prompt_tokens).toBe(10)
    expect(usage.completion_tokens).toBe(5)
  })

  test("include_usage=false omits usage field", async () => {
    const sse = makeSSE(
      '{"type":"response.output_text.delta","delta":"hi"}',
      '{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}'
    )

    const chunks = await collectStream(sse, false)
    const dataChunks = chunks.filter((c) => c.startsWith("data: {")).map(parseChunk)

    // Content chunks should NOT have usage field
    expect(dataChunks[1].usage).toBeUndefined()
  })
})

describe("createOpenAIStreamTransformer - tool_calls streaming", () => {
  test("streams tool call header then arguments then finish_reason:tool_calls", async () => {
    const sse = makeSSE(
      '{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_abc","name":"get_weather"}}',
      '{"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\\"city\\""}',
      '{"type":"response.function_call_arguments.delta","output_index":0,"delta":":\\"NYC\\"}"}',
      '{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":5,"output_tokens":10,"total_tokens":15}}}'
    )

    const chunks = await collectStream(sse)
    const dataChunks = chunks.filter((c) => c.startsWith("data: {")).map(parseChunk)

    // role chunk
    const role = dataChunks[0]
    expect((role.choices as { delta: { role: string } }[])[0].delta.role).toBe("assistant")

    // tool call header
    const tcHeader = dataChunks[1]
    const tcDelta = (tcHeader.choices as { delta: { tool_calls: { index: number; id: string; function: { name: string } }[] } }[])[0].delta.tool_calls![0]
    expect(tcDelta.index).toBe(0)
    expect(tcDelta.id).toBe("call_abc")
    expect(tcDelta.function.name).toBe("get_weather")

    // arguments deltas
    const argDelta1 = (dataChunks[2] as { choices: { delta: { tool_calls: { function: { arguments: string } }[] } }[] }).choices[0].delta.tool_calls![0]
    expect(argDelta1.function.arguments).toBe('{"city"')

    // finish
    const finish = dataChunks[dataChunks.length - 1]
    expect((finish.choices as { finish_reason: string }[])[0].finish_reason).toBe("tool_calls")

    expect(chunks[chunks.length - 1]).toContain("[DONE]")
  })

  test("handles parallel tool_calls with different output_indexes", async () => {
    const sse = makeSSE(
      '{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"c1","name":"fn1"}}',
      '{"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","call_id":"c2","name":"fn2"}}',
      '{"type":"response.function_call_arguments.delta","output_index":0,"delta":"{}"}',
      '{"type":"response.function_call_arguments.delta","output_index":1,"delta":"{}"}',
      '{"type":"response.completed","response":{"id":"r1"}}'
    )

    const chunks = await collectStream(sse)
    const dataChunks = chunks.filter((c) => c.startsWith("data: {")).map(parseChunk)

    // Find tool call headers
    const tcChunks = dataChunks.filter((c) => {
      const choices = c.choices as { delta: { tool_calls?: unknown[] } }[]
      return choices?.[0]?.delta?.tool_calls
    })

    // Should have at least 4: header1, header2, args1, args2
    expect(tcChunks.length).toBeGreaterThanOrEqual(4)

    // Finish should be tool_calls
    const finish = dataChunks[dataChunks.length - 1]
    expect((finish.choices as { finish_reason: string }[])[0].finish_reason).toBe("tool_calls")
  })
})

describe("createOpenAIStreamTransformer - error handling", () => {
  test("response.failed propagates error", async () => {
    const sse = makeSSE(
      '{"type":"response.failed","response":{"error":{"code":"rate_limit","message":"Too many requests"}}}'
    )

    const encoder = new TextEncoder()
    const ctx = createOpenAIStreamContext("test-model", false)
    const transformer = createOpenAIStreamTransformer(ctx)

    const input = new ReadableStream({
      start(controller) {
        controller.enqueue(encoder.encode(sse))
        controller.close()
      },
    })

    const reader = input
      .pipeThrough(new TextDecoderStream())
      .pipeThrough(transformer)
      .getReader()

    try {
      await reader.read()
      expect(true).toBe(false) // should not reach
    } catch (err) {
      expect((err as Error).message).toBe("Too many requests")
    }
  })

  test("ignores unknown event types", async () => {
    const sse = makeSSE(
      '{"type":"response.some_unknown_event","data":"whatever"}',
      '{"type":"response.output_text.delta","delta":"ok"}',
      '{"type":"response.completed","response":{"id":"r1"}}'
    )

    const chunks = await collectStream(sse)
    const dataChunks = chunks.filter((c) => c.startsWith("data: {"))
    // role + content + finish = 3
    expect(dataChunks.length).toBe(3)
  })
})

describe("collectOpenAIResponse - non-streaming", () => {
  function makeSseBody(sseData: string): ReadableStream<Uint8Array> {
    const encoder = new TextEncoder()
    return new ReadableStream({
      start(controller) {
        controller.enqueue(encoder.encode(sseData))
        controller.close()
      },
    })
  }

  test("collects text response", async () => {
    const sse = makeSSE(
      '{"type":"response.output_text.delta","delta":"Hello "}',
      '{"type":"response.output_text.delta","delta":"world"}',
      '{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}'
    )

    const ctx = createOpenAIStreamContext("gpt-5", false)
    const result = await collectOpenAIResponse(makeSseBody(sse), ctx)
    expect(result.object).toBe("chat.completion")
    expect(result.model).toBe("gpt-5")
    expect(result.system_fingerprint).toBeDefined()

    const choices = result.choices as { message: { role: string; content: string }; finish_reason: string }[]
    expect(choices[0].message.role).toBe("assistant")
    expect(choices[0].message.content).toBe("Hello world")
    expect(choices[0].finish_reason).toBe("stop")

    const usage = result.usage as { prompt_tokens: number; completion_tokens: number; total_tokens: number }
    expect(usage.prompt_tokens).toBe(10)
    expect(usage.completion_tokens).toBe(5)
  })

  test("collects tool_calls response", async () => {
    const sse = makeSSE(
      '{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_1","name":"search"}}',
      '{"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\\"q\\":\\"test\\"}"}',
      '{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\\"q\\":\\"test\\"}"}',
      '{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":5,"output_tokens":8,"total_tokens":13}}}'
    )

    const ctx = createOpenAIStreamContext("gpt-5", false)
    const result = await collectOpenAIResponse(makeSseBody(sse), ctx)

    const choices = result.choices as {
      message: { role: string; content: string | null; tool_calls: { id: string; function: { name: string; arguments: string } }[] }
      finish_reason: string
    }[]
    expect(choices[0].message.content).toBeNull()
    expect(choices[0].message.tool_calls).toHaveLength(1)
    expect(choices[0].message.tool_calls[0].id).toBe("call_1")
    expect(choices[0].message.tool_calls[0].function.name).toBe("search")
    expect(choices[0].message.tool_calls[0].function.arguments).toBe('{"q":"test"}')
    expect(choices[0].finish_reason).toBe("tool_calls")
  })

  test("throws on response.failed", async () => {
    const sse = makeSSE(
      '{"type":"response.failed","response":{"error":{"code":"err","message":"Something broke"}}}'
    )

    const ctx = createOpenAIStreamContext("gpt-5", false)
    try {
      await collectOpenAIResponse(makeSseBody(sse), ctx)
      expect(true).toBe(false)
    } catch (err) {
      expect((err as Error).message).toBe("Something broke")
    }
  })

  test("uses output_item.done text when available", async () => {
    const sse = makeSSE(
      '{"type":"response.output_text.delta","delta":"partial"}',
      '{"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"final complete text"}]}}',
      '{"type":"response.completed","response":{"id":"r1"}}'
    )

    const ctx = createOpenAIStreamContext("gpt-5", false)
    const result = await collectOpenAIResponse(makeSseBody(sse), ctx)
    const choices = result.choices as { message: { content: string } }[]
    expect(choices[0].message.content).toBe("final complete text")
  })
})
