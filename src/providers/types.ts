import type { OllamaChatRequest, OllamaGenerateRequest, OllamaModelInfo, OllamaModelDetails } from "../types/ollama"
import type { OpenAIChatRequest } from "../types/openai"

// BackendRequest is intentionally broad — each adapter casts its own typed request.
// Using `object` instead of `Record<string, unknown>` allows TypeScript interfaces
// (like CodexResponsesRequest) to be assigned without explicit casting.
export type BackendRequest = object

export interface BackendClient {
  sendRequest(body: BackendRequest): Promise<Response>
}

export interface BackendConverter {
  resolveModel(model: string): string
  chatRequestToBackend(req: OllamaChatRequest): BackendRequest
  generateRequestToBackend(req: OllamaGenerateRequest): BackendRequest
  openaiChatRequestToBackend(req: OpenAIChatRequest): BackendRequest
}

export interface StreamContext {
  readonly model: string
  readonly startTime: number
}

export interface OpenAIStreamContext {
  readonly completionId: string
  readonly created: number
  readonly model: string
  readonly systemFingerprint: string
  readonly includeUsage: boolean
}

export interface BackendStreaming {
  createChatStreamTransformer(ctx: StreamContext): TransformStream<string, string>
  createGenerateStreamTransformer(ctx: StreamContext): TransformStream<string, string>
  collectResponse(body: ReadableStream<Uint8Array>): Promise<string>
  createOpenAIStreamContext(model: string, includeUsage: boolean): OpenAIStreamContext
  createOpenAIStreamTransformer(ctx: OpenAIStreamContext): TransformStream<string, string>
  collectOpenAIResponse(body: ReadableStream<Uint8Array>, ctx: OpenAIStreamContext): Promise<Record<string, unknown>>
}

export interface BackendModels {
  getVisibleModels(): readonly OllamaModelInfo[]
  getAllModels(): readonly OllamaModelInfo[]
  modelExists(name: string): boolean
  createModelDetails(): OllamaModelDetails
}

export interface BackendAdapter {
  readonly name: string
  readonly client: BackendClient
  readonly converter: BackendConverter
  readonly streaming: BackendStreaming
  readonly models: BackendModels
}
