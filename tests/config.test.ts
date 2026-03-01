import { describe, expect, test } from "vitest"
import { loadConfig } from "../src/config"
import { homedir } from "node:os"
import { resolve } from "node:path"

describe("loadConfig", () => {
  test("returns defaults when no env vars set", () => {
    const config = loadConfig({})
    expect(config.port).toBe(11434)
    expect(config.authPath).toBe(resolve(homedir(), ".codex/auth.json"))
    expect(config.logLevel).toBe("info")
    expect(config.chatgptApiUrl).toBe("https://chatgpt.com/backend-api/codex/responses")
    expect(config.backend).toBe("codex")
  })

  test("parses PORT from env", () => {
    const config = loadConfig({ PORT: "8080" })
    expect(config.port).toBe(8080)
  })

  test("throws on invalid PORT", () => {
    expect(() => loadConfig({ PORT: "abc" })).toThrow("Invalid PORT")
    expect(() => loadConfig({ PORT: "0" })).toThrow("Invalid PORT")
    expect(() => loadConfig({ PORT: "99999" })).toThrow("Invalid PORT")
    expect(() => loadConfig({ PORT: "-1" })).toThrow("Invalid PORT")
  })

  test("accepts boundary PORT values", () => {
    expect(loadConfig({ PORT: "1" }).port).toBe(1)
    expect(loadConfig({ PORT: "65535" }).port).toBe(65535)
  })

  test("expands tilde in AUTH_PATH", () => {
    const config = loadConfig({ AUTH_PATH: "~/custom/auth.json" })
    expect(config.authPath).toBe(resolve(homedir(), "custom/auth.json"))
  })

  test("resolves absolute AUTH_PATH", () => {
    const config = loadConfig({ AUTH_PATH: "/tmp/auth.json" })
    expect(config.authPath).toBe("/tmp/auth.json")
  })

  test("parses LOG_LEVEL", () => {
    expect(loadConfig({ LOG_LEVEL: "debug" }).logLevel).toBe("debug")
    expect(loadConfig({ LOG_LEVEL: "WARN" }).logLevel).toBe("warn")
    expect(loadConfig({ LOG_LEVEL: "invalid" }).logLevel).toBe("info")
  })

  test("allows custom CHATGPT_API_URL", () => {
    const config = loadConfig({ CHATGPT_API_URL: "http://localhost:9999/api" })
    expect(config.chatgptApiUrl).toBe("http://localhost:9999/api")
  })

  test("expands bare tilde in AUTH_PATH", () => {
    const config = loadConfig({ AUTH_PATH: "~" })
    expect(config.authPath).toBe(resolve(homedir()))
  })

  test("resolves relative AUTH_PATH", () => {
    const config = loadConfig({ AUTH_PATH: "relative/path.json" })
    expect(config.authPath).toBe(resolve("relative/path.json"))
  })

  test("parses BACKEND env var", () => {
    expect(loadConfig({ BACKEND: "codex" }).backend).toBe("codex")
    expect(loadConfig({ BACKEND: "CODEX" }).backend).toBe("codex")
  })

  test("defaults to codex for unknown BACKEND", () => {
    expect(loadConfig({ BACKEND: "unknown" }).backend).toBe("codex")
    expect(loadConfig({}).backend).toBe("codex")
  })
})
