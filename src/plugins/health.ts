import Elysia from "elysia"

export const healthPlugin = new Elysia()
  .get("/", () => "Ollama is running")
  .head("/", () => new Response(null, { status: 200 }))
