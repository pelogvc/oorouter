import { readFile } from "node:fs/promises"
import type { AuthInfo } from "../../auth"
import { createCodexClient } from "./client"
import type { CodexResponsesRequest, CodexAuth } from "./types"
import { resolveModel, chatRequestToCodex, generateRequestToCodex, openaiChatRequestToCodex } from "./converter"
import { createChatStreamTransformer, createGenerateStreamTransformer, collectSSEResponse } from "./streaming"
import { createOpenAIStreamContext, createOpenAIStreamTransformer, collectOpenAIResponse } from "./openai-streaming"
import { getVisibleModels, getAllModels, modelExists, createModelDetails } from "./models"
import type { BackendAdapter, BackendRequest } from "../types"

export type CodexAdapterOptions = {
  readonly apiUrl: string
} & (
  | { readonly authPath: string; readonly auth?: never }
  | { readonly auth: AuthInfo; readonly authPath?: never }
)

export async function loadCodexAuth(authPath: string): Promise<AuthInfo> {
  let raw: string
  try {
    raw = await readFile(authPath, "utf-8")
  } catch (err) {
    throw new Error(`Failed to read auth file at ${authPath}: ${(err as Error).message}`)
  }

  let auth: CodexAuth
  try {
    auth = JSON.parse(raw) as CodexAuth
  } catch {
    throw new Error(`Invalid JSON in auth file: ${authPath}`)
  }

  if (auth.tokens?.access_token) {
    return {
      mode: "chatgpt",
      accessToken: auth.tokens.access_token,
      accountId: auth.tokens.account_id,
    }
  }

  if (auth.OPENAI_API_KEY) {
    return {
      mode: "api_key",
      accessToken: auth.OPENAI_API_KEY,
    }
  }

  throw new Error(
    "No valid credentials found in auth file. Expected tokens.access_token or OPENAI_API_KEY"
  )
}

export async function createCodexAdapter(options: CodexAdapterOptions): Promise<BackendAdapter> {
  const auth = options.auth ?? await loadCodexAuth(options.authPath!)
  const codexClient = createCodexClient({ auth, apiUrl: options.apiUrl })

  return {
    name: "codex",
    client: {
      sendRequest: (body: BackendRequest) =>
        codexClient.sendRequest(body as CodexResponsesRequest),
    },
    converter: {
      resolveModel,
      chatRequestToBackend: chatRequestToCodex,
      generateRequestToBackend: generateRequestToCodex,
      openaiChatRequestToBackend: openaiChatRequestToCodex,
    },
    streaming: {
      createChatStreamTransformer,
      createGenerateStreamTransformer,
      collectResponse: collectSSEResponse,
      createOpenAIStreamContext,
      createOpenAIStreamTransformer,
      collectOpenAIResponse,
    },
    models: {
      getVisibleModels,
      getAllModels,
      modelExists,
      createModelDetails,
    },
  }
}
