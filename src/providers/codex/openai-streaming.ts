import { parseSSELine } from "./streaming"
import type {
  CodexSSEDeltaEvent,
  CodexSSEOutputItemAdded,
  CodexSSEFunctionCallArgsDelta,
  CodexSSECompletedEvent,
  CodexSSEResponseUsage,
} from "./types"
import type { OpenAIChunk, OpenAIUsage, OpenAIToolCall } from "../../types/openai"

export interface OpenAIStreamContext {
  readonly completionId: string
  readonly created: number
  readonly model: string
  readonly systemFingerprint: string
  readonly includeUsage: boolean
}

interface ToolCallState {
  readonly index: number
  readonly id: string
  readonly name: string
  arguments: string
}

export function createOpenAIStreamContext(
  model: string,
  includeUsage: boolean
): OpenAIStreamContext {
  return {
    completionId: `chatcmpl-${crypto.randomUUID()}`,
    created: Math.floor(Date.now() / 1000),
    model,
    systemFingerprint: `fp_${crypto.randomUUID().replace(/-/g, "").slice(0, 12)}`,
    includeUsage,
  }
}

function buildChunk(
  ctx: OpenAIStreamContext,
  choices: OpenAIChunk["choices"],
  usage?: OpenAIUsage | null
): OpenAIChunk {
  return {
    id: ctx.completionId,
    object: "chat.completion.chunk",
    created: ctx.created,
    model: ctx.model,
    system_fingerprint: ctx.systemFingerprint,
    choices,
    ...(usage !== undefined && { usage }),
  }
}

function formatSSE(chunk: OpenAIChunk): string {
  return `data: ${JSON.stringify(chunk)}\n\n`
}

function mapUsage(codexUsage?: CodexSSEResponseUsage): OpenAIUsage {
  if (!codexUsage) {
    return { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 }
  }
  return {
    prompt_tokens: codexUsage.input_tokens,
    completion_tokens: codexUsage.output_tokens,
    total_tokens: codexUsage.total_tokens,
  }
}

export function createOpenAIStreamTransformer(
  ctx: OpenAIStreamContext
): TransformStream<string, string> {
  let buffer = ""
  let isFirstChunk = true
  const toolCallsByOutputIndex = new Map<number, ToolCallState>()
  let nextToolCallIndex = 0

  return new TransformStream({
    transform(chunk, controller) {
      buffer += chunk
      const lines = buffer.split("\n")
      buffer = lines.pop() ?? ""

      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed) continue

        const event = parseSSELine(trimmed)
        if (!event) continue

        // Text content delta
        if (event.type === "response.output_text.delta") {
          const deltaEvent = event as CodexSSEDeltaEvent
          if (isFirstChunk) {
            const roleChunk = buildChunk(ctx, [
              { index: 0, delta: { role: "assistant" }, finish_reason: null },
            ], ctx.includeUsage ? null : undefined)
            controller.enqueue(formatSSE(roleChunk))
            isFirstChunk = false
          }

          const contentChunk = buildChunk(ctx, [
            { index: 0, delta: { content: deltaEvent.delta }, finish_reason: null },
          ], ctx.includeUsage ? null : undefined)
          controller.enqueue(formatSSE(contentChunk))
        }

        // Tool call start
        if (event.type === "response.output_item.added") {
          const addedEvent = event as CodexSSEOutputItemAdded
          if (addedEvent.item.type === "function_call") {
            const tcIndex = nextToolCallIndex++
            toolCallsByOutputIndex.set(addedEvent.output_index, {
              index: tcIndex,
              id: addedEvent.item.call_id ?? "",
              name: addedEvent.item.name ?? "",
              arguments: "",
            })

            if (isFirstChunk) {
              const roleChunk = buildChunk(ctx, [
                { index: 0, delta: { role: "assistant" }, finish_reason: null },
              ], ctx.includeUsage ? null : undefined)
              controller.enqueue(formatSSE(roleChunk))
              isFirstChunk = false
            }

            const tcChunk = buildChunk(ctx, [
              {
                index: 0,
                delta: {
                  tool_calls: [{
                    index: tcIndex,
                    id: addedEvent.item.call_id ?? "",
                    type: "function",
                    function: { name: addedEvent.item.name ?? "", arguments: "" },
                  }],
                },
                finish_reason: null,
              },
            ], ctx.includeUsage ? null : undefined)
            controller.enqueue(formatSSE(tcChunk))
          }
        }

        // Tool call arguments delta
        if (event.type === "response.function_call_arguments.delta") {
          const argsDelta = event as CodexSSEFunctionCallArgsDelta
          const tc = toolCallsByOutputIndex.get(argsDelta.output_index)
          if (tc) {
            tc.arguments += argsDelta.delta
            const argChunk = buildChunk(ctx, [
              {
                index: 0,
                delta: {
                  tool_calls: [{
                    index: tc.index,
                    function: { arguments: argsDelta.delta },
                  }],
                },
                finish_reason: null,
              },
            ], ctx.includeUsage ? null : undefined)
            controller.enqueue(formatSSE(argChunk))
          }
        }

        // Response completed / done
        if (event.type === "response.completed" || event.type === "response.done") {
          const completedEvent = event as CodexSSECompletedEvent
          const hasToolCalls = toolCallsByOutputIndex.size > 0
          const finishReason = hasToolCalls ? "tool_calls" : "stop"
          const usage = mapUsage(completedEvent.response.usage)

          const finishChunk = buildChunk(ctx, [
            { index: 0, delta: {}, finish_reason: finishReason },
          ], ctx.includeUsage ? usage : undefined)
          controller.enqueue(formatSSE(finishChunk))
          controller.enqueue("data: [DONE]\n\n")
        }

        // Error
        if (event.type === "response.failed") {
          const errEvent = event as { response: { error: { message: string } } }
          controller.error(new Error(errEvent.response.error.message))
        }
      }
    },

    flush(controller) {
      if (buffer.trim()) {
        const event = parseSSELine(buffer.trim())
        if (event?.type === "response.output_text.delta") {
          const deltaEvent = event as CodexSSEDeltaEvent
          const contentChunk = buildChunk(ctx, [
            { index: 0, delta: { content: deltaEvent.delta }, finish_reason: null },
          ], ctx.includeUsage ? null : undefined)
          controller.enqueue(formatSSE(contentChunk))
        }
      }
    },
  })
}

// Non-streaming: collect full SSE response into OpenAI completion object

export async function collectOpenAIResponse(
  body: ReadableStream<Uint8Array>,
  ctx: OpenAIStreamContext
): Promise<Record<string, unknown>> {
  const decoder = new TextDecoder()
  const reader = body.getReader()
  let sseBuffer = ""
  let fullText = ""
  const toolCalls: { index: number; id: string; name: string; arguments: string }[] = []
  const toolCallsByOutputIndex = new Map<number, number>() // output_index → toolCalls array index
  let usage: OpenAIUsage = { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 }

  while (true) {
    const { done, value } = await reader.read()
    if (done) break

    sseBuffer += decoder.decode(value, { stream: true })
    const lines = sseBuffer.split("\n")
    sseBuffer = lines.pop() ?? ""

    for (const line of lines) {
      const trimmed = line.trim()
      if (!trimmed) continue

      const event = parseSSELine(trimmed)
      if (!event) continue

      if (event.type === "response.output_text.delta") {
        fullText += (event as CodexSSEDeltaEvent).delta
      }

      if (event.type === "response.output_item.done") {
        const itemEvent = event as {
          item: { content?: readonly { text: string }[] }
        }
        if (itemEvent.item?.content?.[0]?.text) {
          fullText = itemEvent.item.content[0].text
        }
      }

      if (event.type === "response.output_item.added") {
        const addedEvent = event as CodexSSEOutputItemAdded
        if (addedEvent.item.type === "function_call") {
          const idx = toolCalls.length
          toolCalls.push({
            index: idx,
            id: addedEvent.item.call_id ?? "",
            name: addedEvent.item.name ?? "",
            arguments: "",
          })
          toolCallsByOutputIndex.set(addedEvent.output_index, idx)
        }
      }

      if (event.type === "response.function_call_arguments.delta") {
        const argsDelta = event as CodexSSEFunctionCallArgsDelta
        const tcIdx = toolCallsByOutputIndex.get(argsDelta.output_index)
        if (tcIdx !== undefined) {
          toolCalls[tcIdx] = {
            ...toolCalls[tcIdx],
            arguments: toolCalls[tcIdx].arguments + argsDelta.delta,
          }
        }
      }

      if (event.type === "response.function_call_arguments.done") {
        const argsDone = event as { output_index: number; arguments: string }
        const tcIdx = toolCallsByOutputIndex.get(argsDone.output_index)
        if (tcIdx !== undefined) {
          toolCalls[tcIdx] = {
            ...toolCalls[tcIdx],
            arguments: argsDone.arguments,
          }
        }
      }

      if (event.type === "response.completed" || event.type === "response.done") {
        const completedEvent = event as CodexSSECompletedEvent
        usage = mapUsage(completedEvent.response.usage)
      }

      if (event.type === "response.failed") {
        const errEvent = event as { response: { error: { message: string } } }
        throw new Error(errEvent.response.error.message)
      }
    }
  }

  const hasToolCalls = toolCalls.length > 0
  const formattedToolCalls: OpenAIToolCall[] = toolCalls.map((tc) => ({
    id: tc.id,
    type: "function" as const,
    function: { name: tc.name, arguments: tc.arguments },
  }))

  return {
    id: ctx.completionId,
    object: "chat.completion",
    created: ctx.created,
    model: ctx.model,
    system_fingerprint: ctx.systemFingerprint,
    choices: [
      {
        index: 0,
        message: {
          role: "assistant",
          content: hasToolCalls ? null : fullText,
          ...(hasToolCalls && { tool_calls: formattedToolCalls }),
        },
        finish_reason: hasToolCalls ? "tool_calls" : "stop",
      },
    ],
    usage,
  }
}
