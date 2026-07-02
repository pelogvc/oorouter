// OpenAI API Compatible Types
// Ported from: src/types/openai.ts

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAITool {
    #[serde(rename = "type")]
    pub tool_type: String, // "function"
    pub function: OpenAIToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // "function"
    pub function: OpenAIToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCallDeltaFunction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCallDelta {
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<OpenAIToolCallDeltaFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAITextContentPart {
    #[serde(rename = "type")]
    pub part_type: String, // "text"
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIImageContentPart {
    #[serde(rename = "type")]
    pub part_type: String, // "image_url"
    pub image_url: OpenAIImageUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIContentPart {
    Text(OpenAITextContentPart),
    Image(OpenAIImageContentPart),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIMessageContent {
    Text(String),
    Parts(Vec<OpenAIContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChatMessage {
    pub role: String, // "system" | "user" | "assistant" | "tool"
    pub content: Option<OpenAIMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIStreamOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIStop {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChatRequest {
    pub model: String,
    pub messages: Vec<OpenAIChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<OpenAIStop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<OpenAIStreamOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChoice {
    pub index: u32,
    pub delta: OpenAIDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChunk {
    pub id: String,
    pub object: String, // "chat.completion.chunk"
    pub created: u64,
    pub model: String,
    pub system_fingerprint: String,
    pub choices: Vec<OpenAIChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
}

// Non-streaming response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAICompletionChoice {
    pub index: u32,
    pub message: OpenAIChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChatCompletion {
    pub id: String,
    pub object: String, // "chat.completion"
    pub created: u64,
    pub model: String,
    pub system_fingerprint: String,
    pub choices: Vec<OpenAICompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAIUsage>,
}

// Models list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIModelObject {
    pub id: String,
    pub object: String, // "model"
    pub created: u64,
    pub owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIModelsResponse {
    pub object: String, // "list"
    pub data: Vec<OpenAIModelObject>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_content_string() {
        let json = r#"{"role":"user","content":"hello"}"#;
        let msg: OpenAIChatMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg.content, Some(OpenAIMessageContent::Text(_))));
    }

    #[test]
    fn test_message_content_null() {
        let json = r#"{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"test","arguments":"{}"}}]}"#;
        let msg: OpenAIChatMessage = serde_json::from_str(json).unwrap();
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_some());
    }

    #[test]
    fn test_message_content_parts() {
        let json = r#"{"role":"user","content":[{"type":"text","text":"hello"},{"type":"image_url","image_url":{"url":"data:image/png;base64,abc"}}]}"#;
        let msg: OpenAIChatMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg.content, Some(OpenAIMessageContent::Parts(_))));
    }

    #[test]
    fn test_chunk_serialize() {
        let chunk = OpenAIChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "gpt-5.3-codex".to_string(),
            system_fingerprint: "fp_abc".to_string(),
            choices: vec![OpenAIChoice {
                index: 0,
                delta: OpenAIDelta {
                    role: None,
                    content: Some("hello".to_string()),
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            usage: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("chatcmpl-123"));
        assert!(json.contains("hello"));
    }

    #[test]
    fn test_tool_call_delta() {
        let json = r#"{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}"#;
        let delta: OpenAIToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.index, 0);
        assert_eq!(delta.id, Some("call_abc".to_string()));
    }

    #[test]
    fn test_chat_request_with_tools() {
        let json = r#"{"model":"gpt-5.3-codex","messages":[{"role":"user","content":"What's the weather?"}],"tools":[{"type":"function","function":{"name":"get_weather","description":"Get weather","parameters":{"type":"object","properties":{}}}}]}"#;
        let req: OpenAIChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "gpt-5.3-codex");
        assert!(req.tools.is_some());
        assert_eq!(req.tools.unwrap().len(), 1);
    }

    #[test]
    fn test_chat_request_accepts_extended_openai_fields() {
        let json = r#"{"model":"gpt-5.3-codex","messages":[],"top_p":0.9,"stop":["END"],"max_completion_tokens":128,"parallel_tool_calls":false,"reasoning_effort":"low"}"#;
        let req: OpenAIChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.top_p, Some(0.9));
        assert!(matches!(req.stop, Some(OpenAIStop::Multiple(_))));
        assert_eq!(req.max_completion_tokens, Some(128));
        assert_eq!(req.parallel_tool_calls, Some(false));
        assert_eq!(req.reasoning_effort, Some("low".to_string()));
    }
}
