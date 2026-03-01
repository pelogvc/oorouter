import Elysia from "elysia"
import type { BackendAdapter } from "../providers/types"
import { createChatHandler } from "../routes/chat"
import { createGenerateHandler } from "../routes/generate"
import type { OllamaChatRequest } from "../types/ollama"
import type { OllamaGenerateRequest } from "../types/ollama"

export function createOllamaPlugin(adapter: BackendAdapter) {
  const chatHandler = createChatHandler(adapter)
  const generateHandler = createGenerateHandler(adapter)

  return new Elysia({ prefix: "/api" })
    .onError(({ code, set }) => {
      if (code === "PARSE") {
        set.status = 400
        return { error: "Invalid JSON body" }
      }
    })
    .get("/tags", () => ({ models: adapter.models.getVisibleModels() }))
    .get("/version", () => ({ version: "0.17.4" }))
    .get("/ps", () => ({
      models: adapter.models.getVisibleModels().map((m) => ({
        ...m,
        expires_at: new Date(Date.now() + 5 * 60 * 1000).toISOString(),
        size_vram: 0,
      })),
    }))
    .post("/show", ({ body, set }) => {
      const b = body as Record<string, unknown>
      const rawName = (b.name ?? b.model) as string | undefined
      if (!rawName) {
        set.status = 400
        return { error: "Missing required field: name or model" }
      }
      const modelName = rawName.replace(/:latest$/, "")
      if (!adapter.models.modelExists(modelName)) {
        set.status = 404
        return { error: `model '${modelName}' not found` }
      }
      return {
        modelfile: `FROM ${modelName}`,
        parameters: "",
        template: "{{ .Prompt }}",
        details: adapter.models.createModelDetails(),
        model_info: {
          "general.architecture": "gpt",
          "general.basename": modelName,
          "gpt.context_length": 128000,
        },
        capabilities: ["completion", "tools"],
      }
    })
    .post("/embed", ({ set }) => {
      set.status = 501
      return { error: "Embedding is not supported by this proxy" }
    })
    .post("/chat", async ({ body, set }) => {
      const b = body as Record<string, unknown>
      if (!b.model || !b.messages) {
        set.status = 400
        return { error: "Missing required fields: model, messages" }
      }
      try {
        return await chatHandler(body as unknown as OllamaChatRequest)
      } catch (err) {
        set.status = 502
        return { error: `Backend API error: ${(err as Error).message}` }
      }
    })
    .post("/generate", async ({ body, set }) => {
      const b = body as Record<string, unknown>
      if (!b.model || !b.prompt) {
        set.status = 400
        return { error: "Missing required fields: model, prompt" }
      }
      try {
        return await generateHandler(body as unknown as OllamaGenerateRequest)
      } catch (err) {
        set.status = 502
        return { error: `Backend API error: ${(err as Error).message}` }
      }
    })
    .post("/copy", () => "")
    .delete("/delete", () => "")
    .post("/pull", () => ({ status: "success" }))
    .post("/push", () => ({ status: "success" }))
}
