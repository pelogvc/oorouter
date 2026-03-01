import { describe, expect, test, vi } from "vitest"
import { createGenerateHandler } from "../../src/routes/generate"
import type { BackendAdapter } from "../../src/providers/types"
import { generateRequestToCodex } from "../../src/providers/codex/converter"
import { createGenerateStreamTransformer, collectSSEResponse } from "../../src/providers/codex/streaming"

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
      chatRequestToBackend: vi.fn(),
      generateRequestToBackend: generateRequestToCodex,
      openaiChatRequestToBackend: vi.fn(),
    },
    streaming: {
      createChatStreamTransformer: vi.fn(),
      createGenerateStreamTransformer,
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
      chatRequestToBackend: vi.fn(),
      generateRequestToBackend: generateRequestToCodex,
      openaiChatRequestToBackend: vi.fn(),
    },
    streaming: {
      createChatStreamTransformer: vi.fn(),
      createGenerateStreamTransformer,
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

describe("generate handler", () => {
  test("handles non-streaming response", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"A joke"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
    ].join("\n")

    const handler = createGenerateHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      prompt: "Tell me a joke",
      stream: false,
    })

    expect(result).not.toBeInstanceOf(Response)
    const data = result as Record<string, unknown>
    expect(data.model).toBe("gpt-5")
    expect(data.response).toBe("A joke")
    expect(data.done).toBe(true)
    expect(data.context).toEqual([])
  })

  test("handles streaming response", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"Hello"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
    ].join("\n")

    const handler = createGenerateHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      prompt: "Hi",
    })

    expect(result).toBeInstanceOf(Response)
    const res = result as Response
    expect(res.headers.get("Content-Type")).toBe("application/x-ndjson")

    const text = await res.text()
    const lines = text.trim().split("\n").filter(Boolean)
    const first = JSON.parse(lines[0])
    expect(first.response).toBe("Hello")
  })

  test("throws when backend API fails (non-streaming)", async () => {
    const handler = createGenerateHandler(createFailingAdapter("Timeout"))
    await expect(
      handler({
        model: "gpt-5",
        prompt: "Hi",
        stream: false,
      })
    ).rejects.toThrow("Timeout")
  })

  test("throws when backend API fails (streaming)", async () => {
    const handler = createGenerateHandler(createFailingAdapter("Network error"))
    await expect(
      handler({
        model: "gpt-5",
        prompt: "Hi",
      })
    ).rejects.toThrow("Network error")
  })

  test("non-streaming response includes done_reason and metrics", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"ok"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
    ].join("\n")

    const handler = createGenerateHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      prompt: "Hi",
      stream: false,
    })

    const data = result as Record<string, unknown>
    expect(data.done_reason).toBe("stop")
    expect(data.total_duration).toBeGreaterThanOrEqual(0)
  })
})
