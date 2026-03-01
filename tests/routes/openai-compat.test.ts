import { describe, expect, test, vi } from "vitest"
import { createOpenAIChatHandler } from "../../src/routes/openai-compat"
import type { BackendAdapter } from "../../src/providers/types"
import { resolveModel, openaiChatRequestToCodex } from "../../src/providers/codex/converter"
import { collectSSEResponse } from "../../src/providers/codex/streaming"
import { createOpenAIStreamContext, createOpenAIStreamTransformer, collectOpenAIResponse } from "../../src/providers/codex/openai-streaming"

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
      resolveModel,
      chatRequestToBackend: vi.fn(),
      generateRequestToBackend: vi.fn(),
      openaiChatRequestToBackend: openaiChatRequestToCodex,
    },
    streaming: {
      createChatStreamTransformer: vi.fn(),
      createGenerateStreamTransformer: vi.fn(),
      collectResponse: collectSSEResponse,
      createOpenAIStreamContext,
      createOpenAIStreamTransformer,
      collectOpenAIResponse,
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
      resolveModel,
      chatRequestToBackend: vi.fn(),
      generateRequestToBackend: vi.fn(),
      openaiChatRequestToBackend: openaiChatRequestToCodex,
    },
    streaming: {
      createChatStreamTransformer: vi.fn(),
      createGenerateStreamTransformer: vi.fn(),
      collectResponse: collectSSEResponse,
      createOpenAIStreamContext,
      createOpenAIStreamTransformer,
      collectOpenAIResponse,
    },
    models: {
      getVisibleModels: vi.fn(() => []),
      getAllModels: vi.fn(() => []),
      modelExists: vi.fn(() => false),
      createModelDetails: vi.fn(),
    },
  }
}

describe("openai chat handler", () => {
  test("returns OpenAI-format non-streaming response", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"Hello"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}',
      "",
    ].join("\n")

    const handler = createOpenAIChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
      stream: false,
    })

    expect(result).not.toBeInstanceOf(Response)
    const data = result as Record<string, unknown>
    expect(data.object).toBe("chat.completion")
    expect(data.system_fingerprint).toBeDefined()

    const choices = data.choices as { message: { role: string; content: string }; finish_reason: string }[]
    expect(choices[0].message.role).toBe("assistant")
    expect(choices[0].message.content).toBe("Hello")
    expect(choices[0].finish_reason).toBe("stop")

    const usage = data.usage as { prompt_tokens: number; completion_tokens: number; total_tokens: number }
    expect(usage.prompt_tokens).toBe(10)
    expect(usage.completion_tokens).toBe(5)
  })

  test("returns non-streaming by default when stream is undefined", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"test"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{}}}',
      "",
    ].join("\n")

    const handler = createOpenAIChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
    })

    expect(result).not.toBeInstanceOf(Response)
  })

  test("returns SSE streaming response", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"Hi"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}}',
      "",
    ].join("\n")

    const handler = createOpenAIChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hello" }],
      stream: true,
    })

    expect(result).toBeInstanceOf(Response)
    const res = result as Response
    expect(res.headers.get("Content-Type")).toBe("text/event-stream")

    const text = await res.text()
    expect(text).toContain("data: ")
    expect(text).toContain("[DONE]")

    const dataLines = text.split("\n").filter((l) => l.startsWith("data: {"))
    expect(dataLines.length).toBeGreaterThanOrEqual(3)

    const roleChunk = JSON.parse(dataLines[0].replace("data: ", ""))
    expect(roleChunk.object).toBe("chat.completion.chunk")
    expect(roleChunk.choices[0].delta.role).toBe("assistant")
    expect(roleChunk.system_fingerprint).toBeDefined()

    const contentChunk = JSON.parse(dataLines[1].replace("data: ", ""))
    expect(contentChunk.choices[0].delta.content).toBe("Hi")

    const finishChunk = JSON.parse(dataLines[dataLines.length - 1].replace("data: ", ""))
    expect(finishChunk.choices[0].finish_reason).toBe("stop")

    expect(roleChunk.id).toBe(contentChunk.id)
    expect(roleChunk.created).toBe(contentChunk.created)
    expect(roleChunk.system_fingerprint).toBe(contentChunk.system_fingerprint)
  })

  test("throws on upstream error", async () => {
    const handler = createOpenAIChatHandler(createFailingAdapter("Connection refused"))
    await expect(
      handler({
        model: "gpt-5",
        messages: [{ role: "user", content: "Hi" }],
      })
    ).rejects.toThrow("Connection refused")
  })

  test("non-streaming tool_calls response", async () => {
    const sse = [
      'data: {"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_1","name":"get_weather"}}',
      "",
      'data: {"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\\"city\\":\\"NYC\\"}"}',
      "",
      'data: {"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\\"city\\":\\"NYC\\"}"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":20,"output_tokens":15,"total_tokens":35}}}',
      "",
    ].join("\n")

    const handler = createOpenAIChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "What's the weather?" }],
      tools: [{ type: "function", function: { name: "get_weather", parameters: {} } }],
      stream: false,
    })

    expect(result).not.toBeInstanceOf(Response)
    const data = result as Record<string, unknown>
    const choices = data.choices as {
      message: { content: string | null; tool_calls: { id: string; function: { name: string; arguments: string } }[] }
      finish_reason: string
    }[]
    expect(choices[0].message.content).toBeNull()
    expect(choices[0].message.tool_calls).toHaveLength(1)
    expect(choices[0].message.tool_calls[0].id).toBe("call_1")
    expect(choices[0].message.tool_calls[0].function.name).toBe("get_weather")
    expect(choices[0].finish_reason).toBe("tool_calls")
  })

  test("handles tool message in conversation history", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"It is sunny in NYC."}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}}}',
      "",
    ].join("\n")

    const mockAdapter = createMockAdapter(sse)
    const handler = createOpenAIChatHandler(mockAdapter)
    const result = await handler({
      model: "gpt-5",
      messages: [
        { role: "user", content: "What's the weather?" },
        {
          role: "assistant",
          content: null,
          tool_calls: [{ id: "call_1", type: "function", function: { name: "get_weather", arguments: '{"city":"NYC"}' } }],
        },
        { role: "tool", content: '{"temp":72,"condition":"sunny"}', tool_call_id: "call_1" },
      ],
      stream: false,
    })

    expect(result).not.toBeInstanceOf(Response)
    const data = result as Record<string, unknown>
    const choices = data.choices as { message: { content: string }; finish_reason: string }[]
    expect(choices[0].message.content).toBe("It is sunny in NYC.")
    expect(choices[0].finish_reason).toBe("stop")

    expect(mockAdapter.client.sendRequest).toHaveBeenCalledTimes(1)
    const calls = (mockAdapter.client.sendRequest as unknown as { mock: { calls: unknown[][] } }).mock.calls
    const codexReq = calls[0][0] as {
      input: { type: string }[]
    }
    expect(codexReq.input).toHaveLength(3)
    expect(codexReq.input[0].type).toBe("message")
    expect(codexReq.input[1].type).toBe("function_call")
    expect(codexReq.input[2].type).toBe("function_call_output")
  })

  test("streaming tool_calls with finish_reason:tool_calls", async () => {
    const sse = [
      'data: {"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_x","name":"search"}}',
      "",
      'data: {"type":"response.function_call_arguments.delta","output_index":0,"delta":"{}"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":5,"output_tokens":3,"total_tokens":8}}}',
      "",
    ].join("\n")

    const handler = createOpenAIChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "search something" }],
      tools: [{ type: "function", function: { name: "search", parameters: {} } }],
      stream: true,
    })

    expect(result).toBeInstanceOf(Response)
    const res = result as Response
    const text = await res.text()
    const dataLines = text.split("\n").filter((l) => l.startsWith("data: {"))
    const finishChunk = JSON.parse(dataLines[dataLines.length - 1].replace("data: ", ""))
    expect(finishChunk.choices[0].finish_reason).toBe("tool_calls")
  })

  test("stream_options.include_usage includes usage in streaming", async () => {
    const sse = [
      'data: {"type":"response.output_text.delta","delta":"ok"}',
      "",
      'data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":3,"output_tokens":1,"total_tokens":4}}}',
      "",
    ].join("\n")

    const handler = createOpenAIChatHandler(createMockAdapter(sse))
    const result = await handler({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
      stream: true,
      stream_options: { include_usage: true },
    })

    expect(result).toBeInstanceOf(Response)
    const res = result as Response
    const text = await res.text()
    const dataLines = text.split("\n").filter((l) => l.startsWith("data: {"))

    const contentChunk = JSON.parse(dataLines[1].replace("data: ", ""))
    expect(contentChunk.usage).toBeNull()

    const finishChunk = JSON.parse(dataLines[dataLines.length - 1].replace("data: ", ""))
    expect(finishChunk.usage).toBeDefined()
    expect(finishChunk.usage.prompt_tokens).toBe(3)
    expect(finishChunk.usage.completion_tokens).toBe(1)
  })
})
