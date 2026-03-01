import { homedir } from "node:os"
import { resolve } from "node:path"

export type BackendType = "codex"

export interface Config {
  readonly port: number
  readonly authPath: string
  readonly logLevel: "debug" | "info" | "warn" | "error"
  readonly chatgptApiUrl: string
  readonly backend: BackendType
}

function expandTilde(filePath: string): string {
  if (filePath.startsWith("~/") || filePath === "~") {
    return resolve(homedir(), filePath.slice(2))
  }
  return resolve(filePath)
}

function parseLogLevel(value: string | undefined): Config["logLevel"] {
  const valid = ["debug", "info", "warn", "error"] as const
  const level = value?.toLowerCase()
  if (level && valid.includes(level as Config["logLevel"])) {
    return level as Config["logLevel"]
  }
  return "info"
}

export function loadConfig(env: Record<string, string | undefined> = process.env): Config {
  const port = parseInt(env.PORT ?? "11434", 10)
  if (isNaN(port) || port < 1 || port > 65535) {
    throw new Error(`Invalid PORT: ${env.PORT}`)
  }

  const authPath = expandTilde(env.AUTH_PATH ?? "~/.codex/auth.json")
  const logLevel = parseLogLevel(env.LOG_LEVEL)
  const chatgptApiUrl = env.CHATGPT_API_URL ?? "https://chatgpt.com/backend-api/codex/responses"

  const backend = parseBackend(env.BACKEND)

  return { port, authPath, logLevel, chatgptApiUrl, backend }
}

function parseBackend(value: string | undefined): BackendType {
  const valid: readonly BackendType[] = ["codex"] as const
  const normalized = value?.toLowerCase()
  if (normalized && valid.includes(normalized as BackendType)) {
    return normalized as BackendType
  }
  return "codex"
}
