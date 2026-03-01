import type { Config } from "../config"
import { createCodexAdapter } from "./codex/adapter"
import type { BackendAdapter } from "./types"

export async function createBackendAdapter(config: Config): Promise<BackendAdapter> {
  switch (config.backend) {
    case "codex":
      return createCodexAdapter({ authPath: config.authPath, apiUrl: config.chatgptApiUrl })
    default:
      throw new Error(`Unknown backend: ${config.backend}`)
  }
}
