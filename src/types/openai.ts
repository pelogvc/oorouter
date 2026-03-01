// OpenAI API Compatible Types

// Tool definitions in requests

export interface OpenAIToolFunction {
  readonly name: string
  readonly description?: string
  readonly parameters?: Record<string, unknown>
}

export interface OpenAITool {
  readonly type: "function"
  readonly function: OpenAIToolFunction
}

// Tool calls in responses

export interface OpenAIToolCall {
  readonly id: string
  readonly type: "function"
  readonly function: {
    readonly name: string
    readonly arguments: string
  }
}

export interface OpenAIToolCallDelta {
  readonly index: number
  readonly id?: string
  readonly type?: "function"
  readonly function?: {
    readonly name?: string
    readonly arguments?: string
  }
}

// Chat messages

export interface OpenAIChatMessage {
  readonly role: "system" | "user" | "assistant" | "tool"
  readonly content: string | null
  readonly tool_calls?: readonly OpenAIToolCall[]
  readonly tool_call_id?: string
}

// Chat request

export interface OpenAIStreamOptions {
  readonly include_usage?: boolean
}

export interface OpenAIChatRequest {
  readonly model: string
  readonly messages: readonly OpenAIChatMessage[]
  readonly stream?: boolean
  readonly temperature?: number
  readonly max_tokens?: number
  readonly tools?: readonly OpenAITool[]
  readonly tool_choice?: string | Record<string, unknown>
  readonly stream_options?: OpenAIStreamOptions
}

// Streaming chunk types

export interface OpenAIDelta {
  readonly role?: "assistant"
  readonly content?: string | null
  readonly tool_calls?: readonly OpenAIToolCallDelta[]
}

export interface OpenAIChoice {
  readonly index: number
  readonly delta: OpenAIDelta
  readonly finish_reason: string | null
}

export interface OpenAIUsage {
  readonly prompt_tokens: number
  readonly completion_tokens: number
  readonly total_tokens: number
}

export interface OpenAIChunk {
  readonly id: string
  readonly object: "chat.completion.chunk"
  readonly created: number
  readonly model: string
  readonly system_fingerprint: string
  readonly choices: readonly OpenAIChoice[]
  readonly usage?: OpenAIUsage | null
}
