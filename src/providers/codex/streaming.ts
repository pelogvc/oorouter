import type { CodexSSEEvent } from "./types"

export function parseSSELine(line: string): CodexSSEEvent | null {
  if (!line.startsWith("data: ")) return null
  const data = line.slice(6).trim()
  if (data === "[DONE]") return null

  try {
    return JSON.parse(data) as CodexSSEEvent
  } catch {
    return null
  }
}

interface StreamContext {
  readonly model: string
  readonly startTime: number
}

function createTimestamp(): string {
  return new Date().toISOString()
}

function createFinalMetrics(startTime: number) {
  const totalNs = (Date.now() - startTime) * 1_000_000
  return {
    total_duration: totalNs,
    load_duration: 0,
    prompt_eval_count: 0,
    prompt_eval_duration: 0,
    eval_count: 0,
    eval_duration: totalNs,
  }
}

export function createChatStreamTransformer(
  ctx: StreamContext
): TransformStream<string, string> {
  let buffer = ""

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

        if (event.type === "response.output_text.delta") {
          const ollamaChunk = JSON.stringify({
            model: ctx.model,
            created_at: createTimestamp(),
            message: { role: "assistant", content: event.delta },
            done: false,
          })
          controller.enqueue(ollamaChunk + "\n")
        }

        if (event.type === "response.completed" || event.type === "response.done") {
          const ollamaFinal = JSON.stringify({
            model: ctx.model,
            created_at: createTimestamp(),
            message: { role: "assistant", content: "" },
            done: true,
            done_reason: "stop",
            ...createFinalMetrics(ctx.startTime),
          })
          controller.enqueue(ollamaFinal + "\n")
        }

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
          const ollamaChunk = JSON.stringify({
            model: ctx.model,
            created_at: createTimestamp(),
            message: { role: "assistant", content: event.delta },
            done: false,
          })
          controller.enqueue(ollamaChunk + "\n")
        }
      }
    },
  })
}

export function createGenerateStreamTransformer(
  ctx: StreamContext
): TransformStream<string, string> {
  let buffer = ""

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

        if (event.type === "response.output_text.delta") {
          const ollamaChunk = JSON.stringify({
            model: ctx.model,
            created_at: createTimestamp(),
            response: event.delta,
            done: false,
          })
          controller.enqueue(ollamaChunk + "\n")
        }

        if (event.type === "response.completed" || event.type === "response.done") {
          const ollamaFinal = JSON.stringify({
            model: ctx.model,
            created_at: createTimestamp(),
            response: "",
            done: true,
            done_reason: "stop",
            context: [],
            ...createFinalMetrics(ctx.startTime),
          })
          controller.enqueue(ollamaFinal + "\n")
        }

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
          const ollamaChunk = JSON.stringify({
            model: ctx.model,
            created_at: createTimestamp(),
            response: event.delta,
            done: false,
          })
          controller.enqueue(ollamaChunk + "\n")
        }
      }
    },
  })
}

export async function collectSSEResponse(body: ReadableStream<Uint8Array>): Promise<string> {
  const decoder = new TextDecoder()
  const reader = body.getReader()
  let buffer = ""
  let fullText = ""

  while (true) {
    const { done, value } = await reader.read()
    if (done) break

    buffer += decoder.decode(value, { stream: true })
    const lines = buffer.split("\n")
    buffer = lines.pop() ?? ""

    for (const line of lines) {
      const trimmed = line.trim()
      if (!trimmed) continue

      const event = parseSSELine(trimmed)
      if (!event) continue

      if (event.type === "response.output_text.delta") {
        fullText += event.delta
      }

      if (event.type === "response.output_item.done") {
        const itemEvent = event as {
          item: { content: readonly { text: string }[] }
        }
        if (itemEvent.item?.content?.[0]?.text) {
          fullText = itemEvent.item.content[0].text
        }
      }

      if (event.type === "response.failed") {
        const errEvent = event as { response: { error: { message: string } } }
        throw new Error(errEvent.response.error.message)
      }
    }
  }

  return fullText
}
