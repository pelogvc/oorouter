import type { BackendAdapter } from "../providers/types"
import type { OpenAIChatRequest } from "../types/openai"

export function createOpenAIChatHandler(adapter: BackendAdapter) {
  return async (body: OpenAIChatRequest): Promise<Record<string, unknown> | Response> => {
    const backendReq = adapter.converter.openaiChatRequestToBackend(body)
    const resolvedModel = adapter.converter.resolveModel(body.model)
    const includeUsage = body.stream_options?.include_usage ?? false

    const response = await adapter.client.sendRequest(backendReq)
    const ctx = adapter.streaming.createOpenAIStreamContext(resolvedModel, includeUsage)

    if (body.stream === false || body.stream === undefined) {
      return await adapter.streaming.collectOpenAIResponse(response.body!, ctx)
    }

    const transformer = adapter.streaming.createOpenAIStreamTransformer(ctx)
    const encoder = new TextEncoder()

    const sseStream = response
      .body!.pipeThrough(new TextDecoderStream())
      .pipeThrough(transformer)
      .pipeThrough(
        new TransformStream<string, Uint8Array>({
          transform(chunk, controller) {
            controller.enqueue(encoder.encode(chunk))
          },
        })
      )

    return new Response(sseStream, {
      headers: {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      },
    })
  }
}
