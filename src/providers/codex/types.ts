// Codex Auth & Responses API Types

export interface CodexTokenData {
  readonly access_token: string
  readonly refresh_token: string
  readonly account_id?: string
  readonly id_token?: string
}

export interface CodexAuth {
  readonly auth_mode?: "chatgpt" | "api_key"
  readonly OPENAI_API_KEY?: string
  readonly tokens?: CodexTokenData
  readonly last_refresh?: string
}

// Responses API Request

export interface CodexTextContentItem {
  readonly type: "input_text" | "output_text"
  readonly text: string
}

export interface CodexImageContentItem {
  readonly type: "input_image"
  readonly image_url: string
}

export type CodexContentItem = CodexTextContentItem | CodexImageContentItem

export interface CodexMessageItem {
  readonly type: "message"
  readonly role: "user" | "assistant" | "system"
  readonly content: readonly CodexContentItem[]
}

// Function call items (for tool_calls support)

export interface CodexFunctionCallItem {
  readonly type: "function_call"
  readonly call_id: string
  readonly name: string
  readonly arguments: string
}

export interface CodexFunctionCallOutputItem {
  readonly type: "function_call_output"
  readonly call_id: string
  readonly output: string
}

export type CodexInputItem = CodexMessageItem | CodexFunctionCallItem | CodexFunctionCallOutputItem

export interface CodexResponsesRequest {
  readonly model: string
  readonly instructions: string
  readonly input: readonly CodexInputItem[]
  readonly tools: readonly unknown[]
  readonly tool_choice: string
  readonly parallel_tool_calls: boolean
  readonly store: boolean
  readonly stream: boolean
  readonly include: readonly string[]
  readonly temperature?: number
  readonly max_output_tokens?: number
}

// SSE Event Types

export interface CodexSSEDeltaEvent {
  readonly type: "response.output_text.delta"
  readonly delta: string
}

export interface CodexSSEOutputItemDone {
  readonly type: "response.output_item.done"
  readonly item: {
    readonly type: "message"
    readonly role: "assistant"
    readonly content: readonly {
      readonly type: "output_text"
      readonly text: string
    }[]
  }
}

export interface CodexSSEResponseUsage {
  readonly input_tokens: number
  readonly output_tokens: number
  readonly total_tokens: number
  readonly input_tokens_details?: {
    readonly cached_tokens: number
  }
  readonly output_tokens_details?: {
    readonly reasoning_tokens: number
  }
}

export interface CodexSSECompletedEvent {
  readonly type: "response.completed" | "response.done"
  readonly response: {
    readonly id: string
    readonly usage?: CodexSSEResponseUsage
  }
}

export interface CodexSSEFailedEvent {
  readonly type: "response.failed"
  readonly response: {
    readonly error: {
      readonly code: string
      readonly message: string
    }
  }
}

export interface CodexSSECreatedEvent {
  readonly type: "response.created"
  readonly response: Record<string, unknown>
}

// Function call SSE events

export interface CodexSSEOutputItemAdded {
  readonly type: "response.output_item.added"
  readonly output_index: number
  readonly item: {
    readonly type: "function_call" | "message" | string
    readonly call_id?: string
    readonly name?: string
  }
}

export interface CodexSSEFunctionCallArgsDelta {
  readonly type: "response.function_call_arguments.delta"
  readonly output_index: number
  readonly delta: string
}

export interface CodexSSEFunctionCallArgsDone {
  readonly type: "response.function_call_arguments.done"
  readonly output_index: number
  readonly arguments: string
}

export interface CodexSSEGenericEvent {
  readonly type: string
  readonly [key: string]: unknown
}

export type CodexSSEEvent =
  | CodexSSEDeltaEvent
  | CodexSSEOutputItemDone
  | CodexSSECompletedEvent
  | CodexSSEFailedEvent
  | CodexSSECreatedEvent
  | CodexSSEOutputItemAdded
  | CodexSSEFunctionCallArgsDelta
  | CodexSSEFunctionCallArgsDone
  | CodexSSEGenericEvent
