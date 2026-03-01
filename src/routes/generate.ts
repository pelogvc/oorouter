import type { BackendAdapter } from "../providers/types"
import { createDurationMetrics } from "../utils/metrics"
import type { OllamaGenerateRequest } from "../types/ollama"

export function createGenerateHandler(adapter: BackendAdapter) {
  return async (body: OllamaGenerateRequest): Promise<Record<string, unknown> | Response> => {
    const backendReq = adapter.converter.generateRequestToBackend(body)
    const startTime = Date.now()
    const response = await adapter.client.sendRequest(backendReq)

    if (body.stream === false) {
      const text = await adapter.streaming.collectResponse(response.body!)
      return {
        model: body.model,
        created_at: new Date().toISOString(),
        response: text,
        done: true,
        done_reason: "stop",
        context: [],
        ...createDurationMetrics(startTime),
      }
    }

    const transformer = adapter.streaming.createGenerateStreamTransformer({
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
