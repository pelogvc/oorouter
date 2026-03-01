import { describe, expect, test } from "vitest"
import {
  parseSSELine,
  createChatStreamTransformer,
  createGenerateStreamTransformer,
  collectSSEResponse,
} from "../../src/providers/codex/streaming"

describe("parseSSELine", () => {
  test("parses valid SSE data line", () => {
    const event = parseSSELine('data: {"type":"response.output_text.delta","delta":"Hello"}')
    expect(event).toEqual({ type: "response.output_text.delta", delta: "Hello" })
  })

  test("returns null for [DONE]", () => {
    expect(parseSSELine("data: [DONE]")).toBeNull()
  })

  test("returns null for non-data lines", () => {
    expect(parseSSELine("event: message")).toBeNull()
    expect(parseSSELine(": comment")).toBeNull()
    expect(parseSSELine("")).toBeNull()
  })

  test("returns null for invalid JSON", () => {
    expect(parseSSELine("data: {broken")).toBeNull()
  })

  test("returns null for empty data value", () => {
    expect(parseSSELine("data: ")).toBeNull()
  })
})

async function pipeSSE(
  sseData: string,
  transformer: TransformStream<string, string>
): Promise<string[]> {
  const encoder = new TextEncoder()
  const source = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(encoder.encode(sseData))
      controller.close()
    },
  })

  const decoded = source.pipeThrough(new TextDecoderStream())
  const output = decoded.pipeThrough(transformer)

  const reader = output.getReader()
  const chunks: string[] = []
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    chunks.push(value)
  }
  return chunks
}

describe("createChatStreamTransformer", () => {
  test("transforms SSE delta events to Ollama NDJSON", async () => {
    const transformer = createChatStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const sseData = [
      'data: {"type":"response.output_text.delta","delta":"Hi"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
      "",
    ].join("\n")

    const chunks = await pipeSSE(sseData, transformer)
    expect(chunks).toHaveLength(2)

    const first = JSON.parse(chunks[0].trim())
    expect(first.model).toBe("gpt-5")
    expect(first.message.content).toBe("Hi")
    expect(first.done).toBe(false)

    const last = JSON.parse(chunks[1].trim())
    expect(last.done).toBe(true)
    expect(last.done_reason).toBe("stop")
    expect(last.total_duration).toBeGreaterThanOrEqual(0)
  })
})

describe("createGenerateStreamTransformer", () => {
  test("transforms SSE delta events to Ollama generate NDJSON", async () => {
    const transformer = createGenerateStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const sseData = [
      'data: {"type":"response.output_text.delta","delta":"World"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
      "",
    ].join("\n")

    const chunks = await pipeSSE(sseData, transformer)

    const first = JSON.parse(chunks[0].trim())
    expect(first.response).toBe("World")
    expect(first.done).toBe(false)

    const last = JSON.parse(chunks[1].trim())
    expect(last.done).toBe(true)
    expect(last.context).toEqual([])
  })
})

describe("collectSSEResponse", () => {
  test("collects full text from delta events", async () => {
    const encoder = new TextEncoder()
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          encoder.encode('data: {"type":"response.output_text.delta","delta":"Hello "}\n\n')
        )
        controller.enqueue(
          encoder.encode('data: {"type":"response.output_text.delta","delta":"World"}\n\n')
        )
        controller.enqueue(
          encoder.encode(
            'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}\n\n'
          )
        )
        controller.close()
      },
    })

    const text = await collectSSEResponse(stream)
    expect(text).toBe("Hello World")
  })

  test("uses output_item.done text when available", async () => {
    const encoder = new TextEncoder()
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          encoder.encode('data: {"type":"response.output_text.delta","delta":"partial"}\n\n')
        )
        controller.enqueue(
          encoder.encode(
            'data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Full response"}]}}\n\n'
          )
        )
        controller.close()
      },
    })

    const text = await collectSSEResponse(stream)
    expect(text).toBe("Full response")
  })

  test("throws on response.failed", async () => {
    const encoder = new TextEncoder()
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          encoder.encode(
            'data: {"type":"response.failed","response":{"error":{"code":"rate_limit","message":"Too many requests"}}}\n\n'
          )
        )
        controller.close()
      },
    })

    expect(collectSSEResponse(stream)).rejects.toThrow("Too many requests")
  })

  test("returns empty string on empty stream", async () => {
    const stream = new ReadableStream({
      start(controller) {
        controller.close()
      },
    })
    const text = await collectSSEResponse(stream)
    expect(text).toBe("")
  })

  test("returns empty string when output_item.done has no content", async () => {
    const encoder = new TextEncoder()
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          encoder.encode(
            'data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[]}}\n\n'
          )
        )
        controller.close()
      },
    })
    const text = await collectSSEResponse(stream)
    expect(text).toBe("")
  })
})

describe("createChatStreamTransformer - error handling", () => {
  test("errors on response.failed event", async () => {
    const transformer = createChatStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const sseData = [
      'data: {"type":"response.failed","response":{"error":{"code":"rate_limit","message":"Rate limit exceeded"}}}',
      "",
    ].join("\n")

    try {
      await pipeSSE(sseData, transformer)
      expect(true).toBe(false) // should not reach
    } catch (err) {
      expect((err as Error).message).toBe("Rate limit exceeded")
    }
  })

  test("ignores unknown event types", async () => {
    const transformer = createChatStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const sseData = [
      'data: {"type":"response.created","response":{"id":"r1"}}',
      "",
      'data: {"type":"response.output_text.delta","delta":"Hi"}',
      "",
      'data: {"type":"response.reasoning_text.delta","delta":"thinking..."}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
      "",
    ].join("\n")

    const chunks = await pipeSSE(sseData, transformer)
    expect(chunks).toHaveLength(2) // only delta + completed
  })

  test("handles response.done event same as completed", async () => {
    const transformer = createChatStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const sseData = [
      'data: {"type":"response.output_text.delta","delta":"Ok"}',
      "",
      'data: {"type":"response.done","response":{"id":"r1","usage":{}}}',
      "",
      "",
    ].join("\n")

    const chunks = await pipeSSE(sseData, transformer)
    const last = JSON.parse(chunks[chunks.length - 1].trim())
    expect(last.done).toBe(true)
    expect(last.done_reason).toBe("stop")
  })
})

describe("createGenerateStreamTransformer - error handling", () => {
  test("errors on response.failed event", async () => {
    const transformer = createGenerateStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const sseData = [
      'data: {"type":"response.failed","response":{"error":{"code":"server_error","message":"Internal error"}}}',
      "",
    ].join("\n")

    try {
      await pipeSSE(sseData, transformer)
      expect(true).toBe(false)
    } catch (err) {
      expect((err as Error).message).toBe("Internal error")
    }
  })
})

describe("streaming - chunk splitting", () => {
  test("handles SSE data split across chunks", async () => {
    const transformer = createChatStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const encoder = new TextEncoder()
    // Split the SSE data mid-line
    const part1 = 'data: {"type":"response.output_text.'
    const part2 =
      'delta","delta":"Hello"}\n\ndata: {"type":"response.completed","response":{"id":"r1","usage":{}}}\n\n'

    const source = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode(part1))
        controller.enqueue(encoder.encode(part2))
        controller.close()
      },
    })

    const output = source.pipeThrough(new TextDecoderStream()).pipeThrough(transformer)
    const reader = output.getReader()
    const chunks: string[] = []
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      chunks.push(value)
    }

    expect(chunks).toHaveLength(2)
    const first = JSON.parse(chunks[0].trim())
    expect(first.message.content).toBe("Hello")
  })
})

describe("streaming - flush path", () => {
  test("chat transformer flushes buffered delta on stream end", async () => {
    const transformer = createChatStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const encoder = new TextEncoder()
    // Send a delta line WITHOUT trailing newline so it stays in the buffer
    const data = 'data: {"type":"response.output_text.delta","delta":"flushed"}'

    const source = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode(data))
        controller.close()
      },
    })

    const output = source.pipeThrough(new TextDecoderStream()).pipeThrough(transformer)
    const reader = output.getReader()
    const chunks: string[] = []
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      chunks.push(value)
    }

    expect(chunks).toHaveLength(1)
    const parsed = JSON.parse(chunks[0].trim())
    expect(parsed.message.content).toBe("flushed")
    expect(parsed.done).toBe(false)
  })

  test("generate transformer flushes buffered delta on stream end", async () => {
    const transformer = createGenerateStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const encoder = new TextEncoder()
    const data = 'data: {"type":"response.output_text.delta","delta":"gen-flushed"}'

    const source = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode(data))
        controller.close()
      },
    })

    const output = source.pipeThrough(new TextDecoderStream()).pipeThrough(transformer)
    const reader = output.getReader()
    const chunks: string[] = []
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      chunks.push(value)
    }

    expect(chunks).toHaveLength(1)
    const parsed = JSON.parse(chunks[0].trim())
    expect(parsed.response).toBe("gen-flushed")
    expect(parsed.done).toBe(false)
  })

  test("chat transformer flush ignores non-delta events", async () => {
    const transformer = createChatStreamTransformer({
      model: "gpt-5",
      startTime: Date.now(),
    })

    const encoder = new TextEncoder()
    const data = 'data: {"type":"response.created","response":{"id":"r1"}}'

    const source = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode(data))
        controller.close()
      },
    })

    const output = source.pipeThrough(new TextDecoderStream()).pipeThrough(transformer)
    const reader = output.getReader()
    const chunks: string[] = []
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      chunks.push(value)
    }

    expect(chunks).toHaveLength(0)
  })
})
