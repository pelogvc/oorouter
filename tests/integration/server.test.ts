import { describe, expect, test, beforeAll, afterAll } from "vitest"
import type { Server } from "bun"
import { createApp } from "../../src/app"
import { createCodexAdapter } from "../../src/providers/codex/adapter"

let mockCodexServer: Server<unknown>
let app: ReturnType<typeof createApp>
const MOCK_CODEX_PORT = 19877

beforeAll(async () => {
  // Mock Codex SSE server
  mockCodexServer = Bun.serve({
    port: MOCK_CODEX_PORT,
    async fetch() {
      const encoder = new TextEncoder()
      const sseData = [
        'data: {"type":"response.created","response":{"id":"resp-1"}}',
        "",
        'data: {"type":"response.output_text.delta","delta":"Hello "}',
        "",
        'data: {"type":"response.output_text.delta","delta":"from proxy"}',
        "",
        'data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello from proxy"}]}}',
        "",
        'data: {"type":"response.completed","response":{"id":"resp-1","usage":{"input_tokens":10,"output_tokens":3,"total_tokens":13}}}',
        "",
      ].join("\n")

      return new Response(encoder.encode(sseData), {
        headers: { "Content-Type": "text/event-stream" },
      })
    },
  })

  const adapter = await createCodexAdapter({
    auth: { mode: "api_key", accessToken: "test-key" },
    apiUrl: `http://localhost:${MOCK_CODEX_PORT}`,
  })

  app = createApp(adapter)
})

afterAll(() => {
  mockCodexServer?.stop()
})

function handle(path: string, init?: RequestInit): Promise<Response> {
  return app.handle(new Request(`http://localhost${path}`, init))
}

describe("Integration: health check", () => {
  test("GET / returns Ollama is running", async () => {
    const res = await handle("/")
    expect(res.status).toBe(200)
    expect(await res.text()).toBe("Ollama is running")
  })

  test("HEAD / returns 200", async () => {
    const res = await handle("/", { method: "HEAD" })
    expect(res.status).toBe(200)
  })
})

describe("Integration: GET endpoints", () => {
  test("GET /api/version", async () => {
    const res = await handle("/api/version")
    const data = (await res.json()) as { version: string }
    expect(data.version).toBe("0.17.4")
  })

  test("GET /api/tags returns model list", async () => {
    const res = await handle("/api/tags")
    const data = (await res.json()) as { models: { name: string; details: { family: string; format: string } }[] }
    expect(data.models.length).toBeGreaterThan(0)
    expect(data.models[0].name).toContain(":latest")
    expect(data.models[0].details.family).toBe("gpt")
    expect(data.models[0].details.format).toBe("api")
  })

  test("GET /api/tags includes visible models and excludes hidden", async () => {
    const res = await handle("/api/tags")
    const data = (await res.json()) as { models: { name: string }[] }
    const names = data.models.map((m) => m.name)

    expect(names).toContain("gpt-5.3-codex:latest")
    expect(names).toContain("gpt-5.2-codex:latest")
    expect(names).not.toContain("gpt-5:latest")
    expect(names).not.toContain("gpt-5.1-codex:latest")
    expect(names).not.toContain("gpt-5-codex:latest")
  })

  test("GET /api/ps", async () => {
    const res = await handle("/api/ps")
    const data = (await res.json()) as { models: { size_vram: number }[] }
    expect(Array.isArray(data.models)).toBe(true)
  })
})

describe("Integration: POST /api/chat (non-streaming)", () => {
  test("returns complete response", async () => {
    const res = await handle("/api/chat", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "gpt-5",
        messages: [{ role: "user", content: "Hello" }],
        stream: false,
      }),
    })

    expect(res.status).toBe(200)
    const data = (await res.json()) as {
      model: string
      message: { role: string; content: string }
      done: boolean
    }
    expect(data.model).toBe("gpt-5")
    expect(data.message.role).toBe("assistant")
    expect(data.message.content).toBe("Hello from proxy")
    expect(data.done).toBe(true)
  })

  test("returns 400 on missing fields", async () => {
    const res = await handle("/api/chat", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "gpt-5" }),
    })
    expect(res.status).toBe(400)
    const data = (await res.json()) as { error: string }
    expect(data.error).toContain("Missing required fields")
  })
})

describe("Integration: POST /api/chat (streaming)", () => {
  test("returns NDJSON stream", async () => {
    const res = await handle("/api/chat", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "gpt-5",
        messages: [{ role: "user", content: "Hello" }],
      }),
    })

    expect(res.status).toBe(200)
    expect(res.headers.get("Content-Type")).toBe("application/x-ndjson")

    const text = await res.text()
    const lines = text.trim().split("\n").filter(Boolean)
    expect(lines.length).toBeGreaterThanOrEqual(2)

    const parsed = lines.map((l) => JSON.parse(l))
    const deltas = parsed.filter((p) => !p.done)
    const final = parsed.find((p) => p.done)

    expect(deltas.length).toBeGreaterThan(0)
    expect(deltas[0].message.role).toBe("assistant")
    expect(final).toBeDefined()
    expect(final!.done_reason).toBe("stop")
  })
})

describe("Integration: POST /api/generate (non-streaming)", () => {
  test("returns complete response", async () => {
    const res = await handle("/api/generate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "gpt-5",
        prompt: "Hi",
        stream: false,
      }),
    })

    expect(res.status).toBe(200)
    const data = (await res.json()) as { response: string; done: boolean }
    expect(data.response).toBe("Hello from proxy")
    expect(data.done).toBe(true)
  })

  test("returns 400 on missing fields", async () => {
    const res = await handle("/api/generate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "gpt-5" }),
    })
    expect(res.status).toBe(400)
    const data = (await res.json()) as { error: string }
    expect(data.error).toContain("Missing required fields")
  })
})

describe("Integration: POST /api/show", () => {
  test("returns detailed model info", async () => {
    const res = await handle("/api/show", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "gpt-5.3-codex" }),
    })
    expect(res.status).toBe(200)
    const data = (await res.json()) as {
      modelfile: string
      parameters: string
      template: string
      details: { family: string }
      model_info: Record<string, unknown>
      capabilities: string[]
    }
    expect(data.modelfile).toBe("FROM gpt-5.3-codex")
    expect(data.parameters).toBe("")
    expect(data.template).toBe("{{ .Prompt }}")
    expect(data.details.family).toBe("gpt")
    expect(data.model_info["general.architecture"]).toBe("gpt")
    expect(data.model_info["general.basename"]).toBe("gpt-5.3-codex")
    expect(data.model_info["gpt.context_length"]).toBe(128000)
    expect(data.capabilities).toContain("completion")
    expect(data.capabilities).toContain("tools")
  })

  test("strips :latest suffix from model name", async () => {
    const res = await handle("/api/show", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "gpt-5.3-codex:latest" }),
    })
    expect(res.status).toBe(200)
    const data = (await res.json()) as { modelfile: string }
    expect(data.modelfile).toBe("FROM gpt-5.3-codex")
  })

  test("accepts model field (VS Code compat)", async () => {
    const res = await handle("/api/show", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "gpt-5.3-codex" }),
    })
    expect(res.status).toBe(200)
    const data = (await res.json()) as { modelfile: string }
    expect(data.modelfile).toBe("FROM gpt-5.3-codex")
  })

  test("returns 404 for unknown model", async () => {
    const res = await handle("/api/show", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "nonexistent" }),
    })
    expect(res.status).toBe(404)
    const data = (await res.json()) as { error: string }
    expect(data.error).toContain("not found")
  })

  test("returns 400 when name field missing", async () => {
    const res = await handle("/api/show", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    })
    expect(res.status).toBe(400)
  })
})

describe("Integration: stub endpoints", () => {
  test("POST /api/embed returns 501", async () => {
    const res = await handle("/api/embed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "gpt-5", input: "test" }),
    })
    expect(res.status).toBe(501)
  })

  test("POST /api/copy returns 200", async () => {
    const res = await handle("/api/copy", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source: "a", destination: "b" }),
    })
    expect(res.status).toBe(200)
  })

  test("DELETE /api/delete returns 200", async () => {
    const res = await handle("/api/delete", {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "test" }),
    })
    expect(res.status).toBe(200)
  })

  test("POST /api/pull returns success", async () => {
    const res = await handle("/api/pull", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "gpt-5" }),
    })
    const data = (await res.json()) as { status: string }
    expect(data.status).toBe("success")
  })

  test("POST /api/push returns success", async () => {
    const res = await handle("/api/push", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "gpt-5" }),
    })
    const data = (await res.json()) as { status: string }
    expect(data.status).toBe("success")
  })
})

describe("Integration: OpenAI compat", () => {
  test("GET /v1/models returns OpenAI-format model list", async () => {
    const res = await handle("/v1/models")
    expect(res.status).toBe(200)
    const data = (await res.json()) as {
      object: string
      data: { id: string; object: string; owned_by: string }[]
    }
    expect(data.object).toBe("list")
    expect(data.data.length).toBeGreaterThan(0)
    expect(data.data[0].object).toBe("model")
    expect(data.data[0].owned_by).toBe("codex-proxy")
    expect(data.data[0].id).not.toContain(":latest")
  })

  test("POST /v1/chat/completions returns 400 on missing fields", async () => {
    const res = await handle("/v1/chat/completions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "gpt-5" }),
    })
    expect(res.status).toBe(400)
    const data = (await res.json()) as { error: { type: string } }
    expect(data.error.type).toBe("invalid_request_error")
  })

  test("POST /v1/chat/completions non-streaming", async () => {
    const res = await handle("/v1/chat/completions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "gpt-5",
        messages: [{ role: "user", content: "Hello" }],
        stream: false,
      }),
    })
    expect(res.status).toBe(200)
    const data = (await res.json()) as {
      object: string
      choices: { message: { role: string; content: string }; finish_reason: string }[]
    }
    expect(data.object).toBe("chat.completion")
    expect(data.choices[0].message.role).toBe("assistant")
    expect(data.choices[0].message.content).toBe("Hello from proxy")
  })
})

describe("Integration: 404", () => {
  test("unknown path returns 404", async () => {
    const res = await handle("/api/unknown")
    expect(res.status).toBe(404)
  })
})
