import Elysia from "elysia"
import type { BackendAdapter } from "../providers/types"
import { createOpenAIChatHandler } from "../routes/openai-compat"
import type { OpenAIChatRequest } from "../types/openai"

export function createOpenAIPlugin(adapter: BackendAdapter) {
  const openaiChatHandler = createOpenAIChatHandler(adapter)

  return new Elysia({ prefix: "/v1" })
    .onError(({ code, set }) => {
      if (code === "PARSE") {
        set.status = 400
        return { error: { message: "Invalid JSON body", type: "invalid_request_error" } }
      }
    })
    .get("/models", () => ({
      object: "list",
      data: adapter.models.getVisibleModels().map((m) => ({
        id: m.name.replace(/:latest$/, ""),
        object: "model",
        created: Math.floor(Date.now() / 1000),
        owned_by: "codex-proxy",
      })),
    }))
    .post("/chat/completions", async ({ body, set }) => {
      const b = body as Record<string, unknown>
      if (!b.model || !b.messages) {
        set.status = 400
        return { error: { message: "Missing required fields: model, messages", type: "invalid_request_error" } }
      }
      try {
        return await openaiChatHandler(body as unknown as OpenAIChatRequest)
      } catch (err) {
        set.status = 502
        return { error: { message: `Upstream error: ${(err as Error).message}`, type: "server_error" } }
      }
    })
}
