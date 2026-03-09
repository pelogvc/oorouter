import type { AuthInfo } from "../../auth"
import { getAuthHeaders } from "../../auth"
import type { CodexResponsesRequest } from "./types"

export class BackendApiError extends Error {
  readonly statusCode: number
  readonly responseBody: string

  constructor(statusCode: number, responseBody: string) {
    super(`Codex API error ${statusCode}: ${responseBody}`)
    this.name = "BackendApiError"
    this.statusCode = statusCode
    this.responseBody = responseBody
  }
}

const BROWSER_HEADERS: Record<string, string> = {
  "Content-Type": "application/json",
  Accept: "text/event-stream",
  "Accept-Language": "en-US,en;q=0.9",
  Referer: "https://chatgpt.com/",
  Origin: "https://chatgpt.com",
  "Sec-Fetch-Dest": "empty",
  "Sec-Fetch-Mode": "cors",
  "Sec-Fetch-Site": "same-origin",
  "Cache-Control": "no-cache",
  DNT: "1",
  "OpenAI-Beta": "responses=experimental",
  originator: "codex_cli_rs",
  "User-Agent":
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
}

export interface CodexClientOptions {
  readonly auth: AuthInfo
  readonly apiUrl: string
}

export function createCodexClient(options: CodexClientOptions) {
  const { auth, apiUrl } = options
  const authHeaders = getAuthHeaders(auth)
  const sessionId = crypto.randomUUID()

  async function sendRequest(body: CodexResponsesRequest): Promise<Response> {
    const headers: Record<string, string> = {
      ...BROWSER_HEADERS,
      ...authHeaders,
      session_id: sessionId,
    }

    const response = await fetch(apiUrl, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    })

    if (!response.ok) {
      const text = await response.text().catch(() => "")
      throw new BackendApiError(response.status, text)
    }

    return response
  }

  return { sendRequest }
}

export type CodexClient = ReturnType<typeof createCodexClient>
