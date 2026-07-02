// Ollama API Request/Response Types
// Reference: https://github.com/ollama/ollama/blob/main/docs/api.md
// Ported from: src/types/ollama.ts

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<OllamaChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<serde_json::Value>, // string | number
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: OllamaChatMessage,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaGenerateRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<serde_json::Value>, // string | number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaGenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelDetails {
    pub parent_model: String,
    pub format: String,
    pub family: String,
    pub families: Vec<String>,
    pub parameter_size: String,
    pub quantization_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub model: String,
    pub modified_at: String,
    pub size: u64,
    pub digest: String,
    pub details: OllamaModelDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaTagsResponse {
    pub models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaShowRequest {
    /// Ollama spec uses "name", VSCode Copilot sends "model" — accept both
    #[serde(alias = "model")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaShowResponse {
    pub modelfile: String,
    pub parameters: String,
    pub template: String,
    pub details: OllamaModelDetails,
    pub model_info: HashMap<String, serde_json::Value>,
    pub capabilities: Vec<String>,
}

/// `string | string[]` — Ollama embed input can be a single string or array of strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OllamaEmbedInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaEmbedRequest {
    pub model: String,
    pub input: OllamaEmbedInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaPsModel {
    pub name: String,
    pub model: String,
    pub modified_at: String,
    pub size: u64,
    pub digest: String,
    pub details: OllamaModelDetails,
    pub expires_at: String,
    pub size_vram: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaPsResponse {
    pub models: Vec<OllamaPsModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaVersionResponse {
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_roundtrip() {
        let json = r#"{"model":"gpt-5.3-codex","messages":[{"role":"user","content":"hello"}],"stream":false}"#;
        let req: OllamaChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "gpt-5.3-codex");
        assert_eq!(req.messages[0].role, "user");
        assert_eq!(req.messages[0].content, "hello");
        assert_eq!(req.stream, Some(false));
        let serialized = serde_json::to_string(&req).unwrap();
        assert!(serialized.contains("gpt-5.3-codex"));
    }

    #[test]
    fn test_chat_request_minimal() {
        let json = r#"{"model":"llama3","messages":[{"role":"system","content":"You are helpful"},{"role":"user","content":"hi"}]}"#;
        let req: OllamaChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "llama3");
        assert_eq!(req.messages.len(), 2);
        assert!(req.stream.is_none());
        assert!(req.options.is_none());
    }

    #[test]
    fn test_chat_response_done() {
        let json = r#"{
            "model": "gpt-5.3-codex",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {"role": "assistant", "content": "Hello!"},
            "done": true,
            "done_reason": "stop",
            "total_duration": 1000000,
            "eval_count": 10,
            "eval_duration": 500000
        }"#;
        let resp: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.done);
        assert_eq!(resp.done_reason, Some("stop".to_string()));
        assert_eq!(resp.message.content, "Hello!");
    }

    #[test]
    fn test_chat_response_streaming_chunk() {
        let json = r#"{
            "model": "gpt-5.3-codex",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {"role": "assistant", "content": "Hel"},
            "done": false
        }"#;
        let resp: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.done);
        assert!(resp.total_duration.is_none());
    }

    #[test]
    fn test_generate_request_roundtrip() {
        let json =
            r#"{"model":"codex","prompt":"Write code","system":"You are a coder","stream":true}"#;
        let req: OllamaGenerateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "codex");
        assert_eq!(req.prompt, "Write code");
        assert_eq!(req.system, Some("You are a coder".to_string()));
        assert_eq!(req.stream, Some(true));
    }

    #[test]
    fn test_generate_response_with_context() {
        let json = r#"{
            "model": "codex",
            "created_at": "2024-01-01T00:00:00Z",
            "response": "done",
            "done": true,
            "context": [1, 2, 3, 4]
        }"#;
        let resp: OllamaGenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.context, Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn test_options_partial() {
        let json = r#"{"temperature":0.7,"top_p":0.9}"#;
        let opts: OllamaOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.temperature, Some(0.7));
        assert_eq!(opts.top_p, Some(0.9));
        assert!(opts.top_k.is_none());
        assert!(opts.stop.is_none());
    }

    #[test]
    fn test_tags_response() {
        let json = r#"{
            "models": [{
                "name": "llama3:latest",
                "model": "llama3",
                "modified_at": "2024-01-01T00:00:00Z",
                "size": 4000000000,
                "digest": "abc123",
                "details": {
                    "parent_model": "",
                    "format": "gguf",
                    "family": "llama",
                    "families": ["llama"],
                    "parameter_size": "8B",
                    "quantization_level": "Q4_0"
                }
            }]
        }"#;
        let resp: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 1);
        assert_eq!(resp.models[0].name, "llama3:latest");
        assert_eq!(resp.models[0].details.family, "llama");
    }

    #[test]
    fn test_embed_request_single() {
        let json = r#"{"model":"nomic","input":"hello world"}"#;
        let req: OllamaEmbedRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.input, OllamaEmbedInput::Single(ref s) if s == "hello world"));
    }

    #[test]
    fn test_embed_request_multiple() {
        let json = r#"{"model":"nomic","input":["hello","world"]}"#;
        let req: OllamaEmbedRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.input, OllamaEmbedInput::Multiple(ref v) if v.len() == 2));
    }

    #[test]
    fn test_keep_alive_string_or_number() {
        let json_str = r#"{"model":"m","messages":[],"keep_alive":"5m"}"#;
        let req: OllamaChatRequest = serde_json::from_str(json_str).unwrap();
        assert!(req.keep_alive.is_some());

        let json_num = r#"{"model":"m","messages":[],"keep_alive":300}"#;
        let req: OllamaChatRequest = serde_json::from_str(json_num).unwrap();
        assert!(req.keep_alive.is_some());
    }

    #[test]
    fn test_show_request_name_field() {
        let json = r#"{"name":"llama3"}"#;
        let req: OllamaShowRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "llama3");
    }

    #[test]
    fn test_show_request_model_field() {
        // VSCode Copilot sends { model: "..." } instead of { name: "..." }
        let json = r#"{"model":"gpt-5.4:latest"}"#;
        let req: OllamaShowRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "gpt-5.4:latest");
    }
    #[test]
    fn test_version_response() {
        let json = r#"{"version":"0.3.0"}"#;
        let resp: OllamaVersionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.version, "0.3.0");
    }

    #[test]
    fn test_ps_response() {
        let json = r#"{
            "models": [{
                "name": "llama3:latest",
                "model": "llama3",
                "modified_at": "2024-01-01T00:00:00.000Z",
                "size": 4000000000,
                "digest": "abc123",
                "details": {
                    "parent_model": "",
                    "format": "gguf",
                    "family": "llama",
                    "families": ["llama"],
                    "parameter_size": "8B",
                    "quantization_level": "Q4_0"
                },
                "expires_at": "2024-12-31T23:59:59Z",
                "size_vram": 2000000000
            }]
        }"#;
        let resp: OllamaPsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 1);
        assert_eq!(resp.models[0].size_vram, 2000000000);
    }

    #[test]
    fn test_chat_message_with_images() {
        let json = r#"{"role":"user","content":"describe this","images":["base64data"]}"#;
        let msg: OllamaChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.images, Some(vec!["base64data".to_string()]));
    }

    #[test]
    fn test_chat_message_without_images_omits_field() {
        let msg = OllamaChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
            images: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("images"));
    }
}
