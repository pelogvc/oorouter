import Elysia from "elysia"
import { cors } from "@elysiajs/cors"
import type { BackendAdapter } from "./providers/types"
import { healthPlugin } from "./plugins/health"
import { createOllamaPlugin } from "./plugins/ollama"
import { createOpenAIPlugin } from "./plugins/openai"

export function createApp(adapter: BackendAdapter) {
  return new Elysia()
    .use(
      cors({
        origin: "*",
        methods: ["GET", "POST", "DELETE", "OPTIONS", "HEAD"],
        allowedHeaders: [
          "Content-Type",
          "Authorization",
          "Accept",
          "User-Agent",
          "X-Requested-With",
          "OpenAI-Beta",
        ],
      })
    )
    .use(healthPlugin)
    .use(createOllamaPlugin(adapter))
    .use(createOpenAIPlugin(adapter))
    .onError(({ code, set }) => {
      if (code === "NOT_FOUND") {
        set.status = 404
        return { error: "Not found" }
      }
    })
}
