import type { OllamaChatRequest, OllamaGenerateRequest } from "../../types/ollama"
import type { CodexResponsesRequest, CodexInputItem, CodexContentItem } from "./types"
import type { OpenAITool, OpenAIChatMessage, OpenAIChatRequest } from "../../types/openai"

const MODEL_ALIASES: Record<string, string> = {
  codex: "gpt-5.3-codex",
  "codex-mini": "gpt-5.1-codex-mini",
  gpt5: "gpt-5",
}

export function resolveModel(model: string): string {
  const stripped = model.replace(/:latest$/, "")
  return MODEL_ALIASES[stripped] ?? stripped
}

export function chatRequestToCodex(req: OllamaChatRequest): CodexResponsesRequest {
  const resolvedModel = resolveModel(req.model)
  const systemMessages: string[] = []
  const inputItems: CodexInputItem[] = []

  for (const msg of req.messages) {
    if (msg.role === "system") {
      systemMessages.push(msg.content)
      continue
    }

    const content: CodexContentItem[] = [{ type: "input_text", text: msg.content }]

    if (msg.role === "assistant") {
      inputItems.push({
        type: "message",
        role: "assistant",
        content: [{ type: "output_text", text: msg.content }],
      })
    } else {
      inputItems.push({ type: "message", role: msg.role, content })
    }
  }

  return {
    model: resolvedModel,
    instructions: systemMessages.join("\n\n"),
    input: inputItems,
    tools: [],
    tool_choice: "auto",
    parallel_tool_calls: false,
    store: false,
    stream: req.stream !== false,
    include: [],
  }
}

export function generateRequestToCodex(req: OllamaGenerateRequest): CodexResponsesRequest {
  const resolvedModel = resolveModel(req.model)
  const input: CodexInputItem[] = [
    {
      type: "message",
      role: "user",
      content: [{ type: "input_text", text: req.prompt }],
    },
  ]

  return {
    model: resolvedModel,
    instructions: req.system ?? "",
    input,
    tools: [],
    tool_choice: "auto",
    parallel_tool_calls: false,
    store: false,
    stream: req.stream !== false,
    include: [],
  }
}

export { createDurationMetrics } from "../../utils/metrics"

// OpenAI-compatible conversion functions (for /v1/chat/completions with tool_calls)

export function convertToolsToCodex(
  tools: readonly OpenAITool[]
): readonly { type: "function"; name: string; description: string; parameters: Record<string, unknown> }[] {
  return tools.map((tool) => ({
    type: "function" as const,
    name: tool.function.name,
    description: tool.function.description ?? "",
    parameters: tool.function.parameters ?? {},
  }))
}

export function convertOpenAIMessagesToCodex(
  messages: readonly OpenAIChatMessage[]
): { readonly instructions: string; readonly input: readonly CodexInputItem[] } {
  const systemMessages: string[] = []
  const inputItems: CodexInputItem[] = []

  for (const msg of messages) {
    if (msg.role === "system") {
      systemMessages.push(msg.content ?? "")
      continue
    }

    if (msg.role === "assistant" && msg.tool_calls && msg.tool_calls.length > 0) {
      // Assistant message with tool_calls → function_call items
      if (msg.content) {
        inputItems.push({
          type: "message",
          role: "assistant",
          content: [{ type: "output_text", text: msg.content }],
        })
      }
      for (const tc of msg.tool_calls) {
        inputItems.push({
          type: "function_call",
          call_id: tc.id,
          name: tc.function.name,
          arguments: tc.function.arguments,
        })
      }
      continue
    }

    if (msg.role === "tool") {
      inputItems.push({
        type: "function_call_output",
        call_id: msg.tool_call_id ?? "",
        output: msg.content ?? "",
      })
      continue
    }

    if (msg.role === "assistant") {
      inputItems.push({
        type: "message",
        role: "assistant",
        content: [{ type: "output_text", text: msg.content ?? "" }],
      })
      continue
    }

    // user message
    inputItems.push({
      type: "message",
      role: "user",
      content: [{ type: "input_text", text: msg.content ?? "" }],
    })
  }

  return {
    instructions: systemMessages.join("\n\n"),
    input: inputItems,
  }
}

export function openaiChatRequestToCodex(req: OpenAIChatRequest): CodexResponsesRequest {
  const resolvedModel = resolveModel(req.model)
  const { instructions, input } = convertOpenAIMessagesToCodex(req.messages)
  const tools = req.tools ? convertToolsToCodex(req.tools) : []

  return {
    model: resolvedModel,
    instructions,
    input,
    tools,
    tool_choice: typeof req.tool_choice === "string" ? req.tool_choice : "auto",
    parallel_tool_calls: tools.length > 0,
    store: false,
    stream: req.stream !== false,
    include: ["usage"],
    ...(req.temperature !== undefined && { temperature: req.temperature }),
    ...(req.max_tokens !== undefined && { max_output_tokens: req.max_tokens }),
  }
}
