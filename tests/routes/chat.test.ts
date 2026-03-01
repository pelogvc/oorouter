import { describe, expect, test, vi } from "vitest"
import { createChatHandler } from "../../src/routes/chat"
import type { BackendAdapter } from "../../src/providers/types"
import { chatRequestToCodex } from "../../src/providers/codex/converter"
import { createChatStreamTransformer, collectSSEResponse } from "../../src/providers/codex/streaming"

function createMockAdapter(sseData: string): BackendAdapter {
  const encoder = new TextEncoder()
  return {
    name: "test",
    client: {
      sendRequest: vi.fn(async () => {
        const body = new ReadableStream({
          start(controller) {
            controller.enqueue(encoder.encode(sseData))
            controller.close()
          },
        })
        return new Response(body, { status: 200 })
      }),
    },
    converter: {
      resolveModel: (m) => m,
      chatRequestToBackend: chatRequestToCodex,
      generateRequestToBackend: vi.fn(),
      openaiChatRequestToBackend: vi.fn(),
    },
    streaming: {
      createChatStreamTransformer,
      createGenerateStreamTransformer: vi.fn(),
      collectResponse: collectSSEResponse,
      createOpenAIStreamContext: vi.fn(),
      createOpenAIStreamTransformer: vi.fn(),
      collectOpenAIResponse: vi.fn(),
    },
    models: {
      getVisibleModels: vi.fn(() => []),
      getAllModels: vi.fn(() => []),
      modelExists: vi.fn(() => false),
      createModelDetails: vi.fn(),
    },
  }
}

function createFailingAdapter(errorMessage: string): BackendAdapter {
  return {
    name: "test",
    client: {
      sendRequest: vi.fn(async () => {
        throw new Error(errorMessage)
      }),
    },
    converter: {
      resolveModel: (m) => m,
      chatRequestToBackend: chatRequestToCodex,
      generateRequestToBackend: vi.fn(),
      openaiChatRequestToBackend: vi.fn(),
    },
    streaming: {
      createChatStreamTransformer,
      createGenerateStreamTransformer: vi.fn(),
      collectResponse: collectSSEResponse,
      createOpenAIStreamContext: vi.fn(),
      createOpenAIStreamTransformer: vi.fn(),
      collectOpenAIResponse: vi.fn(),
    },
    models: {
      getVisibleModels: vi.fn(() => []),
      getAllModels: vi.fn(() => []),
      modelExists: vi.fn(() => false),
      createModelDetails: vi.fn(),
    },
  }
}

describe("chat handler", () => {
  test("handles non-streaming response", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"Hello World"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
    ].join("\n")

    const handler = createChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
      stream: false,
    })

    expect(result).not.toBeInstanceOf(Response)
    const data = result as Record<string, unknown>
    expect(data.model).toBe("gpt-5")
    const message = data.message as { role: string; content: string }
    expect(message.role).toBe("assistant")
    expect(message.content).toBe("Hello World")
    expect(data.done).toBe(true)
    expect(data.done_reason).toBe("stop")
  })

  test("handles streaming response", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"Hi"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
    ].join("\n")

    const handler = createChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hello" }],
    })

    expect(result).toBeInstanceOf(Response)
    const res = result as Response
    expect(res.headers.get("Content-Type")).toBe("application/x-ndjson")

    const text = await res.text()
    const lines = text.trim().split("\n").filter(Boolean)
    expect(lines.length).toBeGreaterThanOrEqual(1)

    const first = JSON.parse(lines[0])
    expect(first.message.content).toBe("Hi")

    const last = JSON.parse(lines[lines.length - 1])
    expect(last.done).toBe(true)
    expect(last.done_reason).toBe("stop")
    expect(last.total_duration).toBeGreaterThanOrEqual(0)
  })

  test("throws when backend API fails", async () => {
    const handler = createChatHandler(createFailingAdapter("Connection refused"))
    await expect(
      handler({
        model: "gpt-5",
        messages: [{ role: "user", content: "Hi" }],
        stream: false,
      })
    ).rejects.toThrow("Connection refused")
  })

  test("throws on streaming backend API failure", async () => {
    const handler = createChatHandler(createFailingAdapter("Timeout"))
    await expect(
      handler({
        model: "gpt-5",
        messages: [{ role: "user", content: "Hello" }],
      })
    ).rejects.toThrow("Timeout")
  })

  test("non-streaming response includes duration metrics", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"test"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
    ].join("\n")

    const handler = createChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
      stream: false,
    })

    const data = result as Record<string, unknown>
    expect(data.total_duration).toBeGreaterThanOrEqual(0)
    expect(data.load_duration).toBe(0)
  })
})
