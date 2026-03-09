import Elysia from "elysia"
import type { BackendAdapter } from "../providers/types"
import { BackendApiError } from "../providers/codex/client"
import { setErrorDetail } from "./logger"
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
    .post("/chat/completions", async ({ body, set, request }) => {
      const b = body as Record<string, unknown>
      if (!b.model || !b.messages) {
        set.status = 400
        return { error: { message: "Missing required fields: model, messages", type: "invalid_request_error" } }
      }
      try {
        return await openaiChatHandler(body as unknown as OpenAIChatRequest)
      } catch (err) {
        if (err instanceof BackendApiError) {
          const { statusCode } = err
          const detail = `Upstream ${statusCode}: ${err.responseBody.slice(0, 1000)}`
          setErrorDetail(request, detail)
          if (statusCode === 401 || statusCode === 403) {
            set.status = 401
            return { error: { message: `Authentication failed (upstream ${statusCode})`, type: "authentication_error" } }
          }
          if (statusCode === 429) {
            set.status = 429
            return { error: { message: "Rate limited by upstream API", type: "rate_limit_error" } }
          }
          set.status = 502
          return { error: { message: `Upstream error ${statusCode}: ${err.responseBody.slice(0, 1000)}`, type: "server_error" } }
        }
        setErrorDetail(request, (err as Error).message)
        set.status = 502
        return { error: { message: `Upstream error: ${(err as Error).message}`, type: "server_error" } }
      }
    })
}
