// Ollama API Request/Response Types
// Reference: https://github.com/ollama/ollama/blob/main/docs/api.md

export interface OllamaChatMessage {
  readonly role: "system" | "user" | "assistant"
  readonly content: string
  readonly images?: readonly string[]
}

export interface OllamaChatRequest {
  readonly model: string
  readonly messages: readonly OllamaChatMessage[]
  readonly stream?: boolean
  readonly format?: string
  readonly options?: OllamaOptions
  readonly keep_alive?: string | number
}

export interface OllamaChatResponse {
  readonly model: string
  readonly created_at: string
  readonly message: OllamaChatMessage
  readonly done: boolean
  readonly done_reason?: string
  readonly total_duration?: number
  readonly load_duration?: number
  readonly prompt_eval_count?: number
  readonly prompt_eval_duration?: number
  readonly eval_count?: number
  readonly eval_duration?: number
}

export interface OllamaGenerateRequest {
  readonly model: string
  readonly prompt: string
  readonly system?: string
  readonly stream?: boolean
  readonly format?: string
  readonly context?: readonly number[]
  readonly options?: OllamaOptions
  readonly keep_alive?: string | number
  readonly images?: readonly string[]
}

export interface OllamaGenerateResponse {
  readonly model: string
  readonly created_at: string
  readonly response: string
  readonly done: boolean
  readonly done_reason?: string
  readonly context?: readonly number[]
  readonly total_duration?: number
  readonly load_duration?: number
  readonly prompt_eval_count?: number
  readonly prompt_eval_duration?: number
  readonly eval_count?: number
  readonly eval_duration?: number
}

export interface OllamaOptions {
  readonly temperature?: number
  readonly top_p?: number
  readonly top_k?: number
  readonly num_predict?: number
  readonly stop?: readonly string[]
  readonly seed?: number
  readonly num_ctx?: number
}

export interface OllamaModelDetails {
  readonly parent_model: string
  readonly format: string
  readonly family: string
  readonly families: readonly string[]
  readonly parameter_size: string
  readonly quantization_level: string
}

export interface OllamaModelInfo {
  readonly name: string
  readonly model: string
  readonly modified_at: string
  readonly size: number
  readonly digest: string
  readonly details: OllamaModelDetails
}

export interface OllamaTagsResponse {
  readonly models: readonly OllamaModelInfo[]
}

export interface OllamaShowRequest {
  readonly name: string
}

export interface OllamaShowResponse {
  readonly modelfile: string
  readonly parameters: string
  readonly template: string
  readonly details: OllamaModelDetails
  readonly model_info: Record<string, unknown>
  readonly capabilities: readonly string[]
}

export interface OllamaEmbedRequest {
  readonly model: string
  readonly input: string | readonly string[]
}

export interface OllamaPsModel {
  readonly name: string
  readonly model: string
  readonly size: number
  readonly digest: string
  readonly details: OllamaModelDetails
  readonly expires_at: string
  readonly size_vram: number
}

export interface OllamaPsResponse {
  readonly models: readonly OllamaPsModel[]
}

export interface OllamaVersionResponse {
  readonly version: string
}
