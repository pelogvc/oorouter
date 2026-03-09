import { describe, expect, test } from "vitest"
import {
  chatRequestToCodex,
  generateRequestToCodex,
  resolveModel,
  createDurationMetrics,
  convertToolsToCodex,
  convertOpenAIMessagesToCodex,
  openaiChatRequestToCodex,
} from "../../src/providers/codex/converter"

describe("resolveModel", () => {
  test("resolves known aliases", () => {
    expect(resolveModel("codex")).toBe("gpt-5.3-codex")
    expect(resolveModel("spark")).toBe("gpt-5.3-codex-spark")
    expect(resolveModel("gpt5")).toBe("gpt-5.4")
  })

  test("passes through unknown models", () => {
    expect(resolveModel("gpt-5.2-codex")).toBe("gpt-5.2-codex")
    expect(resolveModel("custom-model")).toBe("custom-model")
  })

  test("strips :latest suffix before resolving", () => {
    expect(resolveModel("gpt-5.3-codex:latest")).toBe("gpt-5.3-codex")
    expect(resolveModel("codex:latest")).toBe("gpt-5.3-codex")
    expect(resolveModel("custom-model:latest")).toBe("custom-model")
  })

  test("passes through empty string", () => {
    expect(resolveModel("")).toBe("")
  })
})

describe("chatRequestToCodex", () => {
  test("converts basic chat request", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hello!" }],
    })

    expect(result.model).toBe("gpt-5")
    expect(result.instructions).toBe("")
    expect(result.input).toEqual([
      {
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Hello!" }],
      },
    ])
    expect(result.stream).toBe(true)
  })

  test("extracts system message to instructions", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [
        { role: "system", content: "You are helpful." },
        { role: "user", content: "Hi" },
      ],
    })

    expect(result.instructions).toBe("You are helpful.")
    expect(result.input).toHaveLength(1)
    expect(result.input[0].role).toBe("user")
  })

  test("joins multiple system messages", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [
        { role: "system", content: "Rule 1" },
        { role: "system", content: "Rule 2" },
        { role: "user", content: "Hi" },
      ],
    })

    expect(result.instructions).toBe("Rule 1\n\nRule 2")
  })

  test("converts assistant messages with output_text", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [
        { role: "user", content: "Hi" },
        { role: "assistant", content: "Hello!" },
        { role: "user", content: "How are you?" },
      ],
    })

    expect(result.input[1]).toEqual({
      type: "message",
      role: "assistant",
      content: [{ type: "output_text", text: "Hello!" }],
    })
  })

  test("resolves model aliases", () => {
    const result = chatRequestToCodex({
      model: "codex",
      messages: [{ role: "user", content: "Hi" }],
    })
    expect(result.model).toBe("gpt-5.3-codex")
  })

  test("respects stream: false", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
      stream: false,
    })
    expect(result.stream).toBe(false)
  })

  test("handles empty messages array", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [],
    })
    expect(result.input).toEqual([])
    expect(result.instructions).toBe("")
  })

  test("handles system-only messages", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [{ role: "system", content: "Instructions only" }],
    })
    expect(result.instructions).toBe("Instructions only")
    expect(result.input).toEqual([])
  })

  test("sets static fields correctly", () => {
    const result = chatRequestToCodex({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
    })
    expect(result.tools).toEqual([])
    expect(result.tool_choice).toBe("auto")
    expect(result.parallel_tool_calls).toBe(false)
    expect(result.store).toBe(false)
    expect(result.include).toEqual([])
  })
})

describe("generateRequestToCodex", () => {
  test("converts basic generate request", () => {
    const result = generateRequestToCodex({
      model: "gpt-5",
      prompt: "Tell me a joke",
    })

    expect(result.model).toBe("gpt-5")
    expect(result.instructions).toBe("")
    expect(result.input).toEqual([
      {
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Tell me a joke" }],
      },
    ])
  })

  test("uses system field as instructions", () => {
    const result = generateRequestToCodex({
      model: "gpt-5",
      prompt: "Hello",
      system: "Be concise",
    })
    expect(result.instructions).toBe("Be concise")
  })

  test("defaults stream to true when unset", () => {
    const result = generateRequestToCodex({
      model: "gpt-5",
      prompt: "Hello",
    })
    expect(result.stream).toBe(true)
  })

  test("defaults instructions to empty string without system", () => {
    const result = generateRequestToCodex({
      model: "gpt-5",
      prompt: "Hello",
    })
    expect(result.instructions).toBe("")
  })

  test("resolves model aliases", () => {
    const result = generateRequestToCodex({
      model: "codex",
      prompt: "Hello",
    })
    expect(result.model).toBe("gpt-5.3-codex")
  })
})

describe("createDurationMetrics", () => {
  test("returns nanosecond metrics with all fields", () => {
    const start = Date.now() - 100
    const metrics = createDurationMetrics(start)
    expect(metrics.total_duration).toBeGreaterThan(0)
    expect(metrics.load_duration).toBe(0)
    expect(metrics.prompt_eval_count).toBe(0)
    expect(metrics.prompt_eval_duration).toBe(0)
    expect(metrics.eval_count).toBe(0)
    expect(metrics.eval_duration).toBe(metrics.total_duration)
  })

  test("converts milliseconds to nanoseconds", () => {
    const start = Date.now() - 50
    const metrics = createDurationMetrics(start)
    // At least 50ms = 50,000,000ns
    expect(metrics.total_duration).toBeGreaterThanOrEqual(50_000_000)
  })
})

describe("convertToolsToCodex", () => {
  test("flattens nested function structure", () => {
    const tools = [
      {
        type: "function" as const,
        function: {
          name: "get_weather",
          description: "Get weather info",
          parameters: { type: "object", properties: { city: { type: "string" } } },
        },
      },
    ]
    const result = convertToolsToCodex(tools)
    expect(result).toEqual([
      {
        type: "function",
        name: "get_weather",
        description: "Get weather info",
        parameters: { type: "object", properties: { city: { type: "string" } } },
      },
    ])
  })

  test("handles empty tools array", () => {
    expect(convertToolsToCodex([])).toEqual([])
  })

  test("handles tool without description", () => {
    const tools = [
      {
        type: "function" as const,
        function: { name: "do_thing" },
      },
    ]
    const result = convertToolsToCodex(tools)
    expect(result[0].description).toBe("")
    expect(result[0].parameters).toEqual({})
  })
})

describe("convertOpenAIMessagesToCodex", () => {
  test("converts assistant message with tool_calls", () => {
    const { input } = convertOpenAIMessagesToCodex([
      { role: "user", content: "What is the weather?", tool_call_id: undefined },
      {
        role: "assistant",
        content: null,
        tool_calls: [
          { id: "call_1", type: "function", function: { name: "get_weather", arguments: '{"city":"NYC"}' } },
        ],
      },
    ])
    expect(input).toHaveLength(2)
    expect(input[1]).toEqual({
      type: "function_call",
      call_id: "call_1",
      name: "get_weather",
      arguments: '{"city":"NYC"}',
    })
  })

  test("converts role:tool to function_call_output", () => {
    const { input } = convertOpenAIMessagesToCodex([
      { role: "tool", content: '{"temp":72}', tool_call_id: "call_1" },
    ])
    expect(input[0]).toEqual({
      type: "function_call_output",
      call_id: "call_1",
      output: '{"temp":72}',
    })
  })

  test("handles mixed conversation with tool_calls", () => {
    const { instructions, input } = convertOpenAIMessagesToCodex([
      { role: "system", content: "You are helpful." },
      { role: "user", content: "Get weather" },
      {
        role: "assistant",
        content: null,
        tool_calls: [
          { id: "c1", type: "function", function: { name: "weather", arguments: "{}" } },
        ],
      },
      { role: "tool", content: "sunny", tool_call_id: "c1" },
      { role: "assistant", content: "It's sunny!" },
    ])
    expect(instructions).toBe("You are helpful.")
    expect(input).toHaveLength(4)
    expect(input[0].type).toBe("message")
    expect(input[1].type).toBe("function_call")
    expect(input[2].type).toBe("function_call_output")
    expect(input[3].type).toBe("message")
  })

  test("handles parallel tool_calls", () => {
    const { input } = convertOpenAIMessagesToCodex([
      {
        role: "assistant",
        content: null,
        tool_calls: [
          { id: "c1", type: "function", function: { name: "fn1", arguments: "{}" } },
          { id: "c2", type: "function", function: { name: "fn2", arguments: "{}" } },
        ],
      },
    ])
    expect(input).toHaveLength(2)
    expect(input[0]).toEqual({ type: "function_call", call_id: "c1", name: "fn1", arguments: "{}" })
    expect(input[1]).toEqual({ type: "function_call", call_id: "c2", name: "fn2", arguments: "{}" })
  })
})

describe("openaiChatRequestToCodex", () => {
  test("creates request with tools", () => {
    const result = openaiChatRequestToCodex({
      model: "codex",
      messages: [{ role: "user", content: "Hi" }],
      tools: [
        { type: "function", function: { name: "test_fn", description: "A test", parameters: {} } },
      ],
    })
    expect(result.model).toBe("gpt-5.3-codex")
    expect(result.tools).toHaveLength(1)
    expect((result.tools[0] as { name: string }).name).toBe("test_fn")
    expect(result.parallel_tool_calls).toBe(true)
    expect(result.include).toEqual([])
  })

  test("creates request without tools", () => {
    const result = openaiChatRequestToCodex({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
      stream: false,
    })
    expect(result.tools).toEqual([])
    expect(result.parallel_tool_calls).toBe(false)
    expect(result.stream).toBe(false)
  })

  test("strips unsupported parameters (temperature, max_tokens)", () => {
    const result = openaiChatRequestToCodex({
      model: "gpt-5",
      messages: [{ role: "user", content: "Hi" }],
      temperature: 0.7,
      max_tokens: 500,
    })
    expect(result).not.toHaveProperty("temperature")
    expect(result).not.toHaveProperty("max_output_tokens")
  })
})
