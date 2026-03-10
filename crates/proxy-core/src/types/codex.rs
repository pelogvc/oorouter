// Codex Auth & Responses API Types
// Ported from: src/providers/codex/types.ts

use serde::{Deserialize, Serialize};

// Auth types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub id_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAuth {
    pub auth_mode: Option<String>,
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    pub tokens: Option<CodexTokenData>,
    pub last_refresh: Option<String>,
}

// Content items
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CodexContentItem {
    #[serde(rename = "input_text")]
    InputText { text: String },
    #[serde(rename = "output_text")]
    OutputText { text: String },
    #[serde(rename = "input_image")]
    InputImage { image_url: String },
}

// Input items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexMessageItem {
    #[serde(rename = "type")]
    pub item_type: String, // "message"
    pub role: String, // "user" | "assistant" | "system"
    pub content: Vec<CodexContentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexFunctionCallItem {
    #[serde(rename = "type")]
    pub item_type: String, // "function_call"
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexFunctionCallOutputItem {
    #[serde(rename = "type")]
    pub item_type: String, // "function_call_output"
    pub call_id: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodexInputItem {
    Message(CodexMessageItem),
    FunctionCall(CodexFunctionCallItem),
    FunctionCallOutput(CodexFunctionCallOutputItem),
}

// Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexResponsesRequest {
    pub model: String,
    pub instructions: String,
    pub input: Vec<CodexInputItem>,
    pub tools: Vec<serde_json::Value>,
    pub tool_choice: String,
    pub parallel_tool_calls: bool,
    pub store: bool,
    pub stream: bool,
    pub include: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

// SSE Events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEDeltaEvent {
    #[serde(rename = "type")]
    pub event_type: String, // "response.output_text.delta"
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEOutputItemContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEOutputItemDoneItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub role: Option<String>,
    pub content: Option<Vec<CodexSSEOutputItemContent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEOutputItemDone {
    #[serde(rename = "type")]
    pub event_type: String, // "response.output_item.done"
    pub item: CodexSSEOutputItemDoneItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEResponseUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub input_tokens_details: Option<serde_json::Value>,
    pub output_tokens_details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEResponseData {
    pub id: Option<String>,
    pub usage: Option<CodexSSEResponseUsage>,
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSECompletedEvent {
    #[serde(rename = "type")]
    pub event_type: String, // "response.completed" | "response.done"
    pub response: CodexSSEResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEFailedEventError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEFailedEventResponse {
    pub error: CodexSSEFailedEventError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEFailedEvent {
    #[serde(rename = "type")]
    pub event_type: String, // "response.failed"
    pub response: CodexSSEFailedEventResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSECreatedEvent {
    #[serde(rename = "type")]
    pub event_type: String, // "response.created"
    pub response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEOutputItemAddedItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub call_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEOutputItemAdded {
    #[serde(rename = "type")]
    pub event_type: String, // "response.output_item.added"
    pub output_index: u32,
    pub item: CodexSSEOutputItemAddedItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEFunctionCallArgsDelta {
    #[serde(rename = "type")]
    pub event_type: String, // "response.function_call_arguments.delta"
    pub output_index: u32,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEFunctionCallArgsDone {
    #[serde(rename = "type")]
    pub event_type: String, // "response.function_call_arguments.done"
    pub output_index: u32,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSSEGenericEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// Main SSE event enum - custom deserializer based on "type" field
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum CodexSSEEvent {
    Delta(CodexSSEDeltaEvent),
    OutputItemDone(CodexSSEOutputItemDone),
    Completed(CodexSSECompletedEvent),
    Failed(CodexSSEFailedEvent),
    Created(CodexSSECreatedEvent),
    OutputItemAdded(CodexSSEOutputItemAdded),
    FunctionCallArgsDelta(CodexSSEFunctionCallArgsDelta),
    FunctionCallArgsDone(CodexSSEFunctionCallArgsDone),
    Generic(CodexSSEGenericEvent),
}

impl<'de> serde::Deserialize<'de> for CodexSSEEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let event_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match event_type {
            "response.output_text.delta" => {
                serde_json::from_value(value).map(CodexSSEEvent::Delta).map_err(serde::de::Error::custom)
            }
            "response.output_item.done" => {
                serde_json::from_value(value).map(CodexSSEEvent::OutputItemDone).map_err(serde::de::Error::custom)
            }
            "response.completed" | "response.done" => {
                serde_json::from_value(value).map(CodexSSEEvent::Completed).map_err(serde::de::Error::custom)
            }
            "response.failed" => {
                serde_json::from_value(value).map(CodexSSEEvent::Failed).map_err(serde::de::Error::custom)
            }
            "response.created" => {
                serde_json::from_value(value).map(CodexSSEEvent::Created).map_err(serde::de::Error::custom)
            }
            "response.output_item.added" => {
                serde_json::from_value(value).map(CodexSSEEvent::OutputItemAdded).map_err(serde::de::Error::custom)
            }
            "response.function_call_arguments.delta" => {
                serde_json::from_value(value).map(CodexSSEEvent::FunctionCallArgsDelta).map_err(serde::de::Error::custom)
            }
            "response.function_call_arguments.done" => {
                serde_json::from_value(value).map(CodexSSEEvent::FunctionCallArgsDone).map_err(serde::de::Error::custom)
            }
            _ => {
                serde_json::from_value(value).map(CodexSSEEvent::Generic).map_err(serde::de::Error::custom)
            }
        }
    }
}
impl CodexSSEEvent {
    pub fn event_type(&self) -> &str {
        match self {
            CodexSSEEvent::Delta(e) => &e.event_type,
            CodexSSEEvent::OutputItemDone(e) => &e.event_type,
            CodexSSEEvent::Completed(e) => &e.event_type,
            CodexSSEEvent::Failed(e) => &e.event_type,
            CodexSSEEvent::Created(e) => &e.event_type,
            CodexSSEEvent::OutputItemAdded(e) => &e.event_type,
            CodexSSEEvent::FunctionCallArgsDelta(e) => &e.event_type,
            CodexSSEEvent::FunctionCallArgsDone(e) => &e.event_type,
            CodexSSEEvent::Generic(e) => &e.event_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_event_parse() {
        let json = r#"{"type":"response.output_text.delta","delta":"hello"}"#;
        let event: CodexSSEEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexSSEEvent::Delta(_)));
        assert_eq!(event.event_type(), "response.output_text.delta");
    }

    #[test]
    fn test_completed_event_parse() {
        let json = r#"{"type":"response.completed","response":{"id":"resp_123","usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30}}}"#;
        let event: CodexSSEEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexSSEEvent::Completed(_)));
    }

    #[test]
    fn test_failed_event_parse() {
        let json = r#"{"type":"response.failed","response":{"error":{"code":"rate_limit","message":"Too many requests"}}}"#;
        let event: CodexSSEEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexSSEEvent::Failed(_)));
    }

    #[test]
    fn test_function_call_args_delta() {
        let json = r#"{"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"key\":"}"#;
        let event: CodexSSEEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexSSEEvent::FunctionCallArgsDelta(_)));
    }

    #[test]
    fn test_codex_responses_request_serialize() {
        let req = CodexResponsesRequest {
            model: "gpt-5.3-codex".to_string(),
            instructions: "".to_string(),
            input: vec![CodexInputItem::Message(CodexMessageItem {
                item_type: "message".to_string(),
                role: "user".to_string(),
                content: vec![CodexContentItem::InputText {
                    text: "hello".to_string(),
                }],
            })],
            tools: vec![],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
            store: false,
            stream: true,
            include: vec!["usage".to_string()],
            temperature: None,
            max_output_tokens: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("gpt-5.3-codex"));
        assert!(json.contains("input_text"));
    }

    #[test]
    fn test_output_item_done_parse() {
        let json = r#"{"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello!"}]}}"#;
        let event: CodexSSEEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexSSEEvent::OutputItemDone(_)));
    }

    #[test]
    fn test_function_call_args_done() {
        let json = r#"{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"key\":\"value\"}"}"#;
        let event: CodexSSEEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexSSEEvent::FunctionCallArgsDone(_)));
    }

    #[test]
    fn test_output_item_added_function_call() {
        let json = r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_123","name":"get_weather"}}"#;
        let event: CodexSSEEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexSSEEvent::OutputItemAdded(_)));
    }
}
