import { describe, expect, test, vi, afterEach } from "vitest"
import { createCodexClient } from "../../src/providers/codex/client"
import type { CodexResponsesRequest } from "../../src/providers/codex/types"

const EMPTY_BODY: CodexResponsesRequest = {
  model: "gpt-5",
  instructions: "",
  input: [],
  tools: [],
  tool_choice: "auto",
  parallel_tool_calls: false,
  store: false,
  stream: true,
  include: [],
}

describe("createCodexClient", () => {
  const originalFetch = globalThis.fetch

  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  test("sends correct headers for chatgpt mode", async () => {
    let capturedHeaders: Record<string, string> = {}
    let capturedUrl = ""
    let capturedMethod = ""

    globalThis.fetch = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
      capturedUrl = typeof url === "string" ? url : url.toString()
      capturedMethod = init?.method ?? ""
      const h = init?.headers as Record<string, string>
      capturedHeaders = { ...h }
      return new Response("{}", { status: 200 })
    }) as unknown as typeof fetch

    const client = createCodexClient({
      auth: { mode: "chatgpt", accessToken: "test-token", accountId: "acc-id" },
      apiUrl: "https://chatgpt.com/backend-api/codex/responses",
    })

    await client.sendRequest(EMPTY_BODY)

    expect(capturedMethod).toBe("POST")
    expect(capturedUrl).toBe("https://chatgpt.com/backend-api/codex/responses")
    expect(capturedHeaders.Authorization).toBe("Bearer test-token")
    expect(capturedHeaders["ChatGPT-Account-ID"]).toBe("acc-id")
    expect(capturedHeaders["OpenAI-Beta"]).toBe("responses=experimental")
    expect(capturedHeaders.originator).toBe("codex_cli_rs")
    expect(capturedHeaders["Content-Type"]).toBe("application/json")
    expect(capturedHeaders.session_id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
    )
  })

  test("sends correct headers for api_key mode (no account ID)", async () => {
    let capturedHeaders: Record<string, string> = {}

    globalThis.fetch = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      capturedHeaders = { ...(init?.headers as Record<string, string>) }
      return new Response("{}", { status: 200 })
    }) as unknown as typeof fetch

    const client = createCodexClient({
      auth: { mode: "api_key", accessToken: "sk-test-key" },
      apiUrl: "http://localhost:9999",
    })

    await client.sendRequest(EMPTY_BODY)

    expect(capturedHeaders.Authorization).toBe("Bearer sk-test-key")
    expect(capturedHeaders["ChatGPT-Account-ID"]).toBeUndefined()
  })

  test("throws on non-ok response with status code", async () => {
    globalThis.fetch = vi.fn(async () => {
      return new Response("Forbidden", { status: 403 })
    }) as unknown as typeof fetch

    const client = createCodexClient({
      auth: { mode: "chatgpt", accessToken: "bad-token" },
      apiUrl: "https://chatgpt.com/backend-api/codex/responses",
    })

    expect(client.sendRequest(EMPTY_BODY)).rejects.toThrow("Codex API error 403: Forbidden")
  })

  test("throws on 401 unauthorized", async () => {
    globalThis.fetch = vi.fn(async () => {
      return new Response("Unauthorized", { status: 401 })
    }) as unknown as typeof fetch

    const client = createCodexClient({
      auth: { mode: "chatgpt", accessToken: "expired" },
      apiUrl: "https://chatgpt.com/backend-api/codex/responses",
    })

    expect(client.sendRequest(EMPTY_BODY)).rejects.toThrow("Codex API error 401")
  })

  test("throws on 500 server error", async () => {
    globalThis.fetch = vi.fn(async () => {
      return new Response("Internal Server Error", { status: 500 })
    }) as unknown as typeof fetch

    const client = createCodexClient({
      auth: { mode: "api_key", accessToken: "key" },
      apiUrl: "http://localhost:9999",
    })

    expect(client.sendRequest(EMPTY_BODY)).rejects.toThrow("Codex API error 500")
  })

  test("propagates network errors from fetch", async () => {
    globalThis.fetch = vi.fn(async () => {
      throw new Error("DNS resolution failed")
    }) as unknown as typeof fetch

    const client = createCodexClient({
      auth: { mode: "api_key", accessToken: "key" },
      apiUrl: "http://invalid-host:9999",
    })

    expect(client.sendRequest(EMPTY_BODY)).rejects.toThrow("DNS resolution failed")
  })

  test("serializes request body as JSON", async () => {
    let capturedBody = ""

    globalThis.fetch = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      capturedBody = (init?.body as string) ?? ""
      return new Response("{}", { status: 200 })
    }) as unknown as typeof fetch

    const client = createCodexClient({
      auth: { mode: "api_key", accessToken: "key" },
      apiUrl: "http://localhost:9999",
    })

    const body: CodexResponsesRequest = {
      ...EMPTY_BODY,
      model: "gpt-5.2-codex",
      instructions: "Be helpful",
    }

    await client.sendRequest(body)

    const parsed = JSON.parse(capturedBody)
    expect(parsed.model).toBe("gpt-5.2-codex")
    expect(parsed.instructions).toBe("Be helpful")
  })
})
