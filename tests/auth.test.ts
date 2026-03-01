import { describe, expect, test, beforeEach, afterEach } from "vitest"
import { loadCodexAuth } from "../src/providers/codex/adapter"
import { getAuthHeaders } from "../src/auth"
import { writeFile, mkdir, rm } from "node:fs/promises"
import { join } from "node:path"
import { tmpdir } from "node:os"

const testDir = join(tmpdir(), "codex-proxy-test-auth")

beforeEach(async () => {
  await mkdir(testDir, { recursive: true })
})

afterEach(async () => {
  await rm(testDir, { recursive: true, force: true })
})

describe("loadCodexAuth", () => {
  test("loads chatgpt tokens with priority", async () => {
    const authFile = join(testDir, "auth.json")
    await writeFile(
      authFile,
      JSON.stringify({
        OPENAI_API_KEY: "sk-test-key",
        tokens: {
          access_token: "eyJ-chatgpt-token",
          account_id: "acc-123",
          refresh_token: "refresh-abc",
        },
      })
    )

    const auth = await loadCodexAuth(authFile)
    expect(auth.mode).toBe("chatgpt")
    expect(auth.accessToken).toBe("eyJ-chatgpt-token")
    expect(auth.accountId).toBe("acc-123")
  })

  test("falls back to OPENAI_API_KEY", async () => {
    const authFile = join(testDir, "auth.json")
    await writeFile(authFile, JSON.stringify({ OPENAI_API_KEY: "sk-fallback" }))

    const auth = await loadCodexAuth(authFile)
    expect(auth.mode).toBe("api_key")
    expect(auth.accessToken).toBe("sk-fallback")
    expect(auth.accountId).toBeUndefined()
  })

  test("throws when file not found", async () => {
    await expect(loadCodexAuth("/nonexistent/auth.json")).rejects.toThrow("Failed to read auth file")
  })

  test("throws on invalid JSON", async () => {
    const authFile = join(testDir, "bad.json")
    await writeFile(authFile, "not json{")
    await expect(loadCodexAuth(authFile)).rejects.toThrow("Invalid JSON")
  })

  test("throws when no credentials present", async () => {
    const authFile = join(testDir, "empty.json")
    await writeFile(authFile, JSON.stringify({}))
    await expect(loadCodexAuth(authFile)).rejects.toThrow("No valid credentials")
  })

  test("falls back to OPENAI_API_KEY when tokens has no access_token", async () => {
    const authFile = join(testDir, "partial.json")
    await writeFile(
      authFile,
      JSON.stringify({
        OPENAI_API_KEY: "sk-fallback-key",
        tokens: { refresh_token: "refresh-only" },
      })
    )

    const auth = await loadCodexAuth(authFile)
    expect(auth.mode).toBe("api_key")
    expect(auth.accessToken).toBe("sk-fallback-key")
  })

  test("handles chatgpt tokens without account_id", async () => {
    const authFile = join(testDir, "no-account.json")
    await writeFile(
      authFile,
      JSON.stringify({
        tokens: {
          access_token: "eyJ-token",
          refresh_token: "refresh",
        },
      })
    )

    const auth = await loadCodexAuth(authFile)
    expect(auth.mode).toBe("chatgpt")
    expect(auth.accessToken).toBe("eyJ-token")
    expect(auth.accountId).toBeUndefined()
  })
})

describe("getAuthHeaders", () => {
  test("returns bearer + account-id for chatgpt mode", () => {
    const headers = getAuthHeaders({
      mode: "chatgpt",
      accessToken: "token-abc",
      accountId: "acc-123",
    })
    expect(headers.Authorization).toBe("Bearer token-abc")
    expect(headers["ChatGPT-Account-ID"]).toBe("acc-123")
  })

  test("returns only bearer for api_key mode", () => {
    const headers = getAuthHeaders({
      mode: "api_key",
      accessToken: "sk-key",
    })
    expect(headers.Authorization).toBe("Bearer sk-key")
    expect(headers["ChatGPT-Account-ID"]).toBeUndefined()
  })
})
