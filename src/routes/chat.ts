import type { BackendAdapter } from "../providers/types"
import { createDurationMetrics } from "../utils/metrics"
import type { OllamaChatRequest } from "../types/ollama"

export function createChatHandler(adapter: BackendAdapter) {
  return async (body: OllamaChatRequest): Promise<Record<string, unknown> | Response> => {
    const backendReq = adapter.converter.chatRequestToBackend(body)
    const startTime = Date.now()
    const response = await adapter.client.sendRequest(backendReq)

    if (body.stream === false) {
      const text = await adapter.streaming.collectResponse(response.body!)
      return {
        model: body.model,
        created_at: new Date().toISOString(),
        message: { role: "assistant" as const, content: text },
        done: true,
        done_reason: "stop",
        ...createDurationMetrics(startTime),
      }
    }

    const transformer = adapter.streaming.createChatStreamTransformer({
      model: body.model,
      startTime,
    })

    const stream = response
      .body!.pipeThrough(new TextDecoderStream())
      .pipeThrough(transformer)

    const encoder = new TextEncoder()
    const encoded = stream.pipeThrough(
      new TransformStream<string, Uint8Array>({
        transform(chunk, controller) {
          controller.enqueue(encoder.encode(chunk))
        },
      })
    )

    return new Response(encoded, {
      headers: { "Content-Type": "application/x-ndjson" },
    })
  }
}
