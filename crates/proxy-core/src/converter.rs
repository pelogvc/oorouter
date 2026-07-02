// Ollama→Codex and OpenAI→Codex request converters
// Ported from: src/providers/codex/converter.ts

use crate::models::get_model_definition;
use crate::types::codex::{
    CodexContentItem, CodexFunctionCallItem, CodexFunctionCallOutputItem, CodexInputItem,
    CodexMessageItem, CodexResponsesRequest,
};
use crate::types::ollama::{OllamaChatRequest, OllamaGenerateRequest};
use crate::types::openai::{
    OpenAIChatMessage, OpenAIChatRequest, OpenAIContentPart, OpenAIMessageContent, OpenAITool,
};

const MODEL_ALIASES: &[(&str, &str)] = &[
    ("codex", "gpt-5.3-codex"),
    ("spark", "gpt-5.3-codex-spark"),
    ("gpt5", "gpt-5.5"),
];

pub fn resolve_model(model: &str) -> String {
    let stripped = model.trim_end_matches(":latest");
    for (alias, resolved) in MODEL_ALIASES {
        if stripped == *alias {
            return resolved.to_string();
        }
    }
    stripped.to_string()
}

fn model_supports_vision(model: &str) -> bool {
    get_model_definition(model)
        .map(|model| model.supports_vision)
        .unwrap_or_else(|| !model.contains("spark"))
}

fn extract_text(content: &Option<OpenAIMessageContent>) -> String {
    match content {
        None => String::new(),
        Some(OpenAIMessageContent::Text(s)) => s.clone(),
        Some(OpenAIMessageContent::Parts(parts)) => parts
            .iter()
            .filter_map(|p| {
                if let OpenAIContentPart::Text(t) = p {
                    Some(t.text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn convert_user_content(content: &Option<OpenAIMessageContent>) -> Vec<CodexContentItem> {
    match content {
        None => vec![CodexContentItem::InputText {
            text: String::new(),
        }],
        Some(OpenAIMessageContent::Text(s)) => {
            vec![CodexContentItem::InputText { text: s.clone() }]
        }
        Some(OpenAIMessageContent::Parts(parts)) => parts
            .iter()
            .map(|p| match p {
                OpenAIContentPart::Image(img) => CodexContentItem::InputImage {
                    image_url: img.image_url.url.clone(),
                },
                OpenAIContentPart::Text(t) => CodexContentItem::InputText {
                    text: t.text.clone(),
                },
            })
            .collect(),
    }
}

fn strip_images(items: Vec<CodexInputItem>) -> Vec<CodexInputItem> {
    items
        .into_iter()
        .map(|item| {
            if let CodexInputItem::Message(mut msg) = item {
                let filtered: Vec<CodexContentItem> = msg
                    .content
                    .into_iter()
                    .filter(|c| !matches!(c, CodexContentItem::InputImage { .. }))
                    .collect();
                msg.content = if filtered.is_empty() {
                    vec![CodexContentItem::InputText {
                        text: String::new(),
                    }]
                } else {
                    filtered
                };
                CodexInputItem::Message(msg)
            } else {
                item
            }
        })
        .collect()
}

pub fn chat_request_to_codex(req: &OllamaChatRequest) -> CodexResponsesRequest {
    let resolved_model = resolve_model(&req.model);
    let mut system_messages: Vec<String> = Vec::new();
    let mut input_items: Vec<CodexInputItem> = Vec::new();

    for msg in &req.messages {
        if msg.role == "system" {
            system_messages.push(msg.content.clone());
            continue;
        }

        if msg.role == "assistant" {
            input_items.push(CodexInputItem::Message(CodexMessageItem {
                item_type: "message".to_string(),
                role: "assistant".to_string(),
                content: vec![CodexContentItem::OutputText {
                    text: msg.content.clone(),
                }],
            }));
        } else {
            let mut content = vec![CodexContentItem::InputText {
                text: msg.content.clone(),
            }];
            if let Some(images) = &msg.images {
                for img in images {
                    content.push(CodexContentItem::InputImage {
                        image_url: format!("data:image/png;base64,{}", img),
                    });
                }
            }
            input_items.push(CodexInputItem::Message(CodexMessageItem {
                item_type: "message".to_string(),
                role: msg.role.clone(),
                content,
            }));
        }
    }

    let input = if model_supports_vision(&resolved_model) {
        input_items
    } else {
        strip_images(input_items)
    };

    CodexResponsesRequest {
        model: resolved_model,
        instructions: system_messages.join("\n\n"),
        input,
        tools: vec![],
        tool_choice: serde_json::json!("auto"),
        parallel_tool_calls: false,
        store: false,
        stream: true, // ChatGPT backend requires stream=true always
        include: vec![],
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        reasoning: None,
    }
}

pub fn generate_request_to_codex(req: &OllamaGenerateRequest) -> CodexResponsesRequest {
    let resolved_model = resolve_model(&req.model);
    let mut content = vec![CodexContentItem::InputText {
        text: req.prompt.clone(),
    }];
    if let Some(images) = &req.images {
        for img in images {
            content.push(CodexContentItem::InputImage {
                image_url: format!("data:image/png;base64,{}", img),
            });
        }
    }
    let input = vec![CodexInputItem::Message(CodexMessageItem {
        item_type: "message".to_string(),
        role: "user".to_string(),
        content,
    })];

    let input = if model_supports_vision(&resolved_model) {
        input
    } else {
        strip_images(input)
    };

    CodexResponsesRequest {
        model: resolved_model,
        instructions: req.system.clone().unwrap_or_default(),
        input,
        tools: vec![],
        tool_choice: serde_json::json!("auto"),
        parallel_tool_calls: false,
        store: false,
        stream: true, // ChatGPT backend requires stream=true always
        include: vec![],
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        reasoning: None,
    }
}

pub fn convert_tools_to_codex(tools: &[OpenAITool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "name": tool.function.name,
                "description": tool.function.description.as_deref().unwrap_or(""),
                "parameters": tool.function.parameters.as_ref().unwrap_or(&serde_json::json!({})),
            })
        })
        .collect()
}

pub fn convert_openai_messages_to_codex(
    messages: &[OpenAIChatMessage],
) -> (String, Vec<CodexInputItem>) {
    let mut system_messages: Vec<String> = Vec::new();
    let mut input_items: Vec<CodexInputItem> = Vec::new();

    for msg in messages {
        if msg.role == "system" || msg.role == "developer" {
            system_messages.push(extract_text(&msg.content));
            continue;
        }

        if msg.role == "assistant" {
            if let Some(tool_calls) = &msg.tool_calls {
                if !tool_calls.is_empty() {
                    if msg.content.is_some() {
                        let text = extract_text(&msg.content);
                        if !text.is_empty() {
                            input_items.push(CodexInputItem::Message(CodexMessageItem {
                                item_type: "message".to_string(),
                                role: "assistant".to_string(),
                                content: vec![CodexContentItem::OutputText { text }],
                            }));
                        }
                    }
                    for tc in tool_calls {
                        input_items.push(CodexInputItem::FunctionCall(CodexFunctionCallItem {
                            item_type: "function_call".to_string(),
                            call_id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        }));
                    }
                    continue;
                }
            }
            input_items.push(CodexInputItem::Message(CodexMessageItem {
                item_type: "message".to_string(),
                role: "assistant".to_string(),
                content: vec![CodexContentItem::OutputText {
                    text: extract_text(&msg.content),
                }],
            }));
            continue;
        }

        if msg.role == "tool" {
            input_items.push(CodexInputItem::FunctionCallOutput(
                CodexFunctionCallOutputItem {
                    item_type: "function_call_output".to_string(),
                    call_id: msg.tool_call_id.clone().unwrap_or_default(),
                    output: extract_text(&msg.content),
                },
            ));
            continue;
        }

        input_items.push(CodexInputItem::Message(CodexMessageItem {
            item_type: "message".to_string(),
            role: "user".to_string(),
            content: convert_user_content(&msg.content),
        }));
    }

    (system_messages.join("\n\n"), input_items)
}

pub fn openai_chat_request_to_codex(req: &OpenAIChatRequest) -> CodexResponsesRequest {
    let resolved_model = resolve_model(&req.model);
    let (instructions, input) = convert_openai_messages_to_codex(&req.messages);
    let tools = req
        .tools
        .as_deref()
        .map(convert_tools_to_codex)
        .unwrap_or_default();
    let has_tools = !tools.is_empty();

    let input = if model_supports_vision(&resolved_model) {
        input
    } else {
        strip_images(input)
    };

    CodexResponsesRequest {
        model: resolved_model,
        instructions,
        input,
        tools,
        tool_choice: convert_tool_choice_to_codex(req.tool_choice.as_ref()),
        parallel_tool_calls: req.parallel_tool_calls.unwrap_or(has_tools),
        store: false,
        stream: true, // ChatGPT backend requires stream=true; non-streaming is assembled from stream
        include: vec![],
        // Match openai-oauth's Codex transport: sampling fields are accepted by
        // the OpenAI-compatible surface but are not forwarded to this backend.
        temperature: None,
        top_p: None,
        // openai-oauth's Codex transport strips max_output_tokens before sending to the
        // Codex backend, which rejects that Responses field.
        max_output_tokens: None,
        reasoning: req.reasoning_effort.as_ref().map(|effort| {
            serde_json::json!({
                "effort": effort,
            })
        }),
    }
}

fn convert_tool_choice_to_codex(tool_choice: Option<&serde_json::Value>) -> serde_json::Value {
    match tool_choice {
        Some(serde_json::Value::String(choice)) => serde_json::json!(choice),
        Some(serde_json::Value::Object(choice)) => {
            let is_function_choice =
                choice.get("type").and_then(|value| value.as_str()) == Some("function");
            let function_name = choice
                .get("function")
                .and_then(|value| value.as_object())
                .and_then(|function| function.get("name"))
                .and_then(|value| value.as_str())
                .or_else(|| choice.get("name").and_then(|value| value.as_str()));

            if is_function_choice {
                if let Some(name) = function_name {
                    return serde_json::json!({
                        "type": "function",
                        "name": name,
                    });
                }
            }

            serde_json::Value::Object(choice.clone())
        }
        Some(value) => value.clone(),
        None => serde_json::json!("auto"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ollama::{OllamaChatMessage, OllamaChatRequest, OllamaGenerateRequest};

    #[test]
    fn test_resolve_model_alias() {
        assert_eq!(resolve_model("codex"), "gpt-5.3-codex");
        assert_eq!(resolve_model("spark"), "gpt-5.3-codex-spark");
        assert_eq!(resolve_model("gpt5"), "gpt-5.5");
    }

    #[test]
    fn test_resolve_model_latest_suffix() {
        assert_eq!(resolve_model("gpt-5.3-codex:latest"), "gpt-5.3-codex");
    }

    #[test]
    fn test_resolve_model_passthrough() {
        assert_eq!(resolve_model("gpt-5.4"), "gpt-5.4");
    }

    #[test]
    fn test_chat_request_system_message() {
        let req = OllamaChatRequest {
            model: "gpt-5.3-codex".to_string(),
            messages: vec![
                OllamaChatMessage {
                    role: "system".to_string(),
                    content: "You are helpful".to_string(),
                    images: None,
                },
                OllamaChatMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                    images: None,
                },
            ],
            stream: Some(false),
            format: None,
            options: None,
            keep_alive: None,
        };
        let codex = chat_request_to_codex(&req);
        assert_eq!(codex.instructions, "You are helpful");
        assert_eq!(codex.input.len(), 1);
    }

    #[test]
    fn test_generate_request_basic() {
        let req = OllamaGenerateRequest {
            model: "gpt-5.3-codex".to_string(),
            prompt: "Hello world".to_string(),
            system: Some("Be helpful".to_string()),
            stream: Some(false),
            format: None,
            context: None,
            options: None,
            keep_alive: None,
            images: None,
        };
        let codex = generate_request_to_codex(&req);
        assert_eq!(codex.instructions, "Be helpful");
        assert_eq!(codex.input.len(), 1);
    }

    #[test]
    fn test_strip_images_non_vision_model() {
        let req = OllamaChatRequest {
            model: "gpt-5.3-codex-spark".to_string(),
            messages: vec![OllamaChatMessage {
                role: "user".to_string(),
                content: "Look at this".to_string(),
                images: Some(vec!["base64data".to_string()]),
            }],
            stream: None,
            format: None,
            options: None,
            keep_alive: None,
        };
        let codex = chat_request_to_codex(&req);
        if let CodexInputItem::Message(msg) = &codex.input[0] {
            assert!(!msg
                .content
                .iter()
                .any(|c| matches!(c, CodexContentItem::InputImage { .. })));
        }
    }

    #[test]
    fn test_chat_request_with_images_vision_model() {
        let req = OllamaChatRequest {
            model: "gpt-5.3-codex".to_string(),
            messages: vec![OllamaChatMessage {
                role: "user".to_string(),
                content: "Describe this".to_string(),
                images: Some(vec!["imgdata".to_string()]),
            }],
            stream: None,
            format: None,
            options: None,
            keep_alive: None,
        };
        let codex = chat_request_to_codex(&req);
        if let CodexInputItem::Message(msg) = &codex.input[0] {
            assert!(msg
                .content
                .iter()
                .any(|c| matches!(c, CodexContentItem::InputImage { .. })));
        }
    }

    #[test]
    fn test_chat_request_assistant_uses_output_text() {
        let req = OllamaChatRequest {
            model: "gpt-5.3-codex".to_string(),
            messages: vec![
                OllamaChatMessage {
                    role: "user".to_string(),
                    content: "Hi".to_string(),
                    images: None,
                },
                OllamaChatMessage {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                    images: None,
                },
            ],
            stream: None,
            format: None,
            options: None,
            keep_alive: None,
        };
        let codex = chat_request_to_codex(&req);
        assert_eq!(codex.input.len(), 2);
        if let CodexInputItem::Message(msg) = &codex.input[1] {
            assert_eq!(msg.role, "assistant");
            assert!(
                matches!(&msg.content[0], CodexContentItem::OutputText { text } if text == "Hello!")
            );
        }
    }

    #[test]
    fn test_openai_chat_request_basic() {
        let req = OpenAIChatRequest {
            model: "codex".to_string(),
            messages: vec![
                OpenAIChatMessage {
                    role: "system".to_string(),
                    content: Some(OpenAIMessageContent::Text("Be helpful".to_string())),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAIChatMessage {
                    role: "user".to_string(),
                    content: Some(OpenAIMessageContent::Text("Hello".to_string())),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            stream: Some(false),
            temperature: Some(0.7),
            top_p: Some(0.9),
            stop: None,
            max_tokens: Some(100),
            max_completion_tokens: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning_effort: Some("low".to_string()),
            stream_options: None,
        };
        let codex = openai_chat_request_to_codex(&req);
        assert_eq!(codex.model, "gpt-5.3-codex");
        assert_eq!(codex.instructions, "Be helpful");
        assert_eq!(codex.input.len(), 1);
        assert_eq!(codex.temperature, None);
        assert_eq!(codex.top_p, None);
        assert_eq!(
            codex.reasoning,
            Some(serde_json::json!({ "effort": "low" }))
        );
        assert_eq!(codex.max_output_tokens, None);
    }

    #[test]
    fn test_openai_chat_request_with_tool_calls() {
        use crate::types::openai::{OpenAIToolCall, OpenAIToolCallFunction};

        let req = OpenAIChatRequest {
            model: "gpt-5.3-codex".to_string(),
            messages: vec![
                OpenAIChatMessage {
                    role: "user".to_string(),
                    content: Some(OpenAIMessageContent::Text(
                        "What's the weather?".to_string(),
                    )),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAIChatMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(vec![OpenAIToolCall {
                        id: "call_123".to_string(),
                        call_type: "function".to_string(),
                        function: OpenAIToolCallFunction {
                            name: "get_weather".to_string(),
                            arguments: r#"{"city":"Seoul"}"#.to_string(),
                        },
                    }]),
                    tool_call_id: None,
                },
                OpenAIChatMessage {
                    role: "tool".to_string(),
                    content: Some(OpenAIMessageContent::Text("Sunny, 25°C".to_string())),
                    tool_calls: None,
                    tool_call_id: Some("call_123".to_string()),
                },
            ],
            stream: None,
            temperature: None,
            top_p: None,
            stop: None,
            max_tokens: None,
            max_completion_tokens: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning_effort: None,
            stream_options: None,
        };
        let codex = openai_chat_request_to_codex(&req);
        assert_eq!(codex.input.len(), 3);
        assert!(
            matches!(&codex.input[1], CodexInputItem::FunctionCall(fc) if fc.name == "get_weather")
        );
        assert!(
            matches!(&codex.input[2], CodexInputItem::FunctionCallOutput(fco) if fco.output == "Sunny, 25°C")
        );
    }

    #[test]
    fn test_openai_chat_request_to_codex_with_tools_sets_parallel_tool_calls() {
        use crate::types::openai::OpenAIToolFunction;

        let req = OpenAIChatRequest {
            model: "codex".to_string(),
            messages: vec![OpenAIChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Text("Hi".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
            stream: Some(false),
            temperature: None,
            top_p: None,
            stop: None,
            max_tokens: None,
            max_completion_tokens: None,
            tools: Some(vec![OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIToolFunction {
                    name: "search_docs".to_string(),
                    description: Some("Search docs".to_string()),
                    parameters: Some(serde_json::json!({"type": "object"})),
                },
            }]),
            tool_choice: Some(serde_json::Value::String("auto".to_string())),
            parallel_tool_calls: None,
            reasoning_effort: None,
            stream_options: None,
        };

        let codex = openai_chat_request_to_codex(&req);
        assert_eq!(codex.model, "gpt-5.3-codex");
        assert_eq!(codex.tools.len(), 1);
        assert!(codex.parallel_tool_calls);
        assert_eq!(codex.tool_choice, serde_json::json!("auto"));
    }

    #[test]
    fn test_openai_chat_request_to_codex_with_forced_function_tool_choice() {
        use crate::types::openai::OpenAIToolFunction;

        let req = OpenAIChatRequest {
            model: "codex".to_string(),
            messages: vec![OpenAIChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Text("Hi".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
            stream: Some(false),
            temperature: None,
            top_p: None,
            stop: None,
            max_tokens: None,
            max_completion_tokens: None,
            tools: Some(vec![OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIToolFunction {
                    name: "search_docs".to_string(),
                    description: Some("Search docs".to_string()),
                    parameters: Some(serde_json::json!({"type": "object"})),
                },
            }]),
            tool_choice: Some(serde_json::json!({
                "type": "function",
                "function": { "name": "search_docs" }
            })),
            parallel_tool_calls: None,
            reasoning_effort: None,
            stream_options: None,
        };

        let codex = openai_chat_request_to_codex(&req);
        assert_eq!(
            codex.tool_choice,
            serde_json::json!({
                "type": "function",
                "name": "search_docs"
            })
        );
    }

    #[test]
    fn test_openai_chat_request_to_codex_with_flat_function_tool_choice() {
        let req = OpenAIChatRequest {
            model: "codex".to_string(),
            messages: vec![OpenAIChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Text("Hi".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
            stream: Some(false),
            temperature: None,
            top_p: None,
            stop: None,
            max_tokens: None,
            max_completion_tokens: None,
            tools: None,
            tool_choice: Some(serde_json::json!({
                "type": "function",
                "name": "search_docs"
            })),
            parallel_tool_calls: None,
            reasoning_effort: None,
            stream_options: None,
        };

        let codex = openai_chat_request_to_codex(&req);
        assert_eq!(
            codex.tool_choice,
            serde_json::json!({
                "type": "function",
                "name": "search_docs"
            })
        );
    }

    #[test]
    fn test_openai_chat_request_to_codex_preserves_unknown_tool_choice_object() {
        let tool_choice = serde_json::json!({
            "type": "function",
            "function": {}
        });
        let req = OpenAIChatRequest {
            model: "codex".to_string(),
            messages: vec![OpenAIChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Text("Hi".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
            stream: Some(false),
            temperature: None,
            top_p: None,
            stop: None,
            max_tokens: None,
            max_completion_tokens: None,
            tools: None,
            tool_choice: Some(tool_choice.clone()),
            parallel_tool_calls: None,
            reasoning_effort: None,
            stream_options: None,
        };

        let codex = openai_chat_request_to_codex(&req);
        assert_eq!(codex.tool_choice, tool_choice);
    }

    #[test]
    fn test_convert_tools_to_codex() {
        use crate::types::openai::OpenAIToolFunction;

        let tools = vec![OpenAITool {
            tool_type: "function".to_string(),
            function: OpenAIToolFunction {
                name: "get_weather".to_string(),
                description: Some("Get weather info".to_string()),
                parameters: Some(serde_json::json!({"type": "object"})),
            },
        }];
        let codex_tools = convert_tools_to_codex(&tools);
        assert_eq!(codex_tools.len(), 1);
        assert_eq!(codex_tools[0]["name"], "get_weather");
        assert_eq!(codex_tools[0]["description"], "Get weather info");
    }

    #[test]
    fn test_convert_tools_to_codex_defaults_empty_fields() {
        use crate::types::openai::OpenAIToolFunction;

        let tools = vec![OpenAITool {
            tool_type: "function".to_string(),
            function: OpenAIToolFunction {
                name: "do_thing".to_string(),
                description: None,
                parameters: None,
            },
        }];

        let codex_tools = convert_tools_to_codex(&tools);
        assert_eq!(codex_tools[0]["description"], "");
        assert_eq!(codex_tools[0]["parameters"], serde_json::json!({}));
    }

    #[test]
    fn test_generate_request_defaults_stream_to_true() {
        let req = OllamaGenerateRequest {
            model: "gpt-5.3-codex".to_string(),
            prompt: "Hello".to_string(),
            system: None,
            stream: None,
            format: None,
            context: None,
            options: None,
            keep_alive: None,
            images: None,
        };

        let codex = generate_request_to_codex(&req);
        assert!(codex.stream);
    }

    #[test]
    fn test_generate_request_with_images_non_vision() {
        let req = OllamaGenerateRequest {
            model: "gpt-5.3-codex-spark".to_string(),
            prompt: "Describe".to_string(),
            system: None,
            stream: None,
            format: None,
            context: None,
            options: None,
            keep_alive: None,
            images: Some(vec!["imgdata".to_string()]),
        };
        let codex = generate_request_to_codex(&req);
        if let CodexInputItem::Message(msg) = &codex.input[0] {
            assert!(!msg
                .content
                .iter()
                .any(|c| matches!(c, CodexContentItem::InputImage { .. })));
        }
    }

    #[test]
    fn test_openai_multiple_system_messages() {
        let req = OpenAIChatRequest {
            model: "gpt-5.3-codex".to_string(),
            messages: vec![
                OpenAIChatMessage {
                    role: "system".to_string(),
                    content: Some(OpenAIMessageContent::Text("Rule 1".to_string())),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAIChatMessage {
                    role: "system".to_string(),
                    content: Some(OpenAIMessageContent::Text("Rule 2".to_string())),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAIChatMessage {
                    role: "user".to_string(),
                    content: Some(OpenAIMessageContent::Text("Hi".to_string())),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            stream: None,
            temperature: None,
            top_p: None,
            stop: None,
            max_tokens: None,
            max_completion_tokens: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning_effort: None,
            stream_options: None,
        };
        let codex = openai_chat_request_to_codex(&req);
        assert_eq!(codex.instructions, "Rule 1\n\nRule 2");
    }

    #[test]
    fn test_stream_defaults_to_true() {
        let req = OllamaChatRequest {
            model: "gpt-5.3-codex".to_string(),
            messages: vec![OllamaChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
                images: None,
            }],
            stream: None,
            format: None,
            options: None,
            keep_alive: None,
        };
        let codex = chat_request_to_codex(&req);
        assert!(codex.stream);
    }
}
