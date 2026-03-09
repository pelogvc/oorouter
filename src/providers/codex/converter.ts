import type { OllamaChatRequest, OllamaGenerateRequest } from "../../types/ollama"
import type { CodexResponsesRequest, CodexInputItem, CodexContentItem } from "./types"
import type { OpenAITool, OpenAIChatMessage, OpenAIChatRequest, OpenAIMessageContent } from "../../types/openai"
import { getCapabilities } from "./models"

function modelSupportsVision(model: string): boolean {
  return getCapabilities(model).includes("vision")
}


function extractText(content: OpenAIMessageContent): string {
  if (content === null || content === undefined) return ""
  if (typeof content === "string") return content
  return content
    .filter((part): part is { type: "text"; text: string } => part.type === "text")
    .map((part) => part.text)
    .join("\n")
}

function convertUserContent(content: OpenAIMessageContent): readonly CodexContentItem[] {
  if (content === null || content === undefined) return [{ type: "input_text", text: "" }]
  if (typeof content === "string") return [{ type: "input_text", text: content }]
  return content.map((part) => {
    if (part.type === "image_url") {
      return { type: "input_image" as const, image_url: part.image_url.url }
    }
    return { type: "input_text" as const, text: part.text }
  })
}

function stripImages(items: readonly CodexInputItem[]): readonly CodexInputItem[] {
  return items.map((item) => {
    if (item.type !== "message") return item
    const filtered = (item.content as readonly CodexContentItem[]).filter((c) => c.type !== "input_image")
    if (filtered.length === 0) {
      return { ...item, content: [{ type: "input_text" as const, text: "" }] }
    }
    return { ...item, content: filtered }
  })
}

const MODEL_ALIASES: Record<string, string> = {
  codex: "gpt-5.3-codex",
  spark: "gpt-5.3-codex-spark",
  gpt5: "gpt-5.4",
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

    if (msg.role === "assistant") {
      inputItems.push({
        type: "message",
        role: "assistant",
        content: [{ type: "output_text", text: msg.content }],
      })
    } else {
      const content: CodexContentItem[] = [{ type: "input_text", text: msg.content }]
      if (msg.images?.length) {
        for (const img of msg.images) {
          content.push({ type: "input_image", image_url: `data:image/png;base64,${img}` })
        }
      }
      inputItems.push({ type: "message", role: msg.role, content })
    }
  }

  return {
    model: resolvedModel,
    instructions: systemMessages.join("\n\n"),
    input: modelSupportsVision(resolvedModel) ? inputItems : stripImages(inputItems),
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
  const content: CodexContentItem[] = [{ type: "input_text", text: req.prompt }]
  if (req.images?.length) {
    for (const img of req.images) {
      content.push({ type: "input_image", image_url: `data:image/png;base64,${img}` })
    }
  }
  const input: CodexInputItem[] = [
    {
      type: "message",
      role: "user",
      content,
    },
  ]

  return {
    model: resolvedModel,
    instructions: req.system ?? "",
    input: modelSupportsVision(resolvedModel) ? input : stripImages(input),
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
      systemMessages.push(extractText(msg.content))
      continue
    }

    if (msg.role === "assistant" && msg.tool_calls && msg.tool_calls.length > 0) {
      // Assistant message with tool_calls → function_call items
      if (msg.content) {
        inputItems.push({
          type: "message",
          role: "assistant",
          content: [{ type: "output_text", text: extractText(msg.content) }],
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
        output: extractText(msg.content),
      })
      continue
    }

    if (msg.role === "assistant") {
      inputItems.push({
        type: "message",
        role: "assistant",
        content: [{ type: "output_text", text: extractText(msg.content) }],
      })
      continue
    }

    // user message
    inputItems.push({
      type: "message",
      role: "user",
      content: convertUserContent(msg.content),
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
    input: modelSupportsVision(resolvedModel) ? input : stripImages(input),
    tools,
    tool_choice: typeof req.tool_choice === "string" ? req.tool_choice : "auto",
    parallel_tool_calls: tools.length > 0,
    store: false,
    stream: req.stream !== false,
    include: [],
  }
}
