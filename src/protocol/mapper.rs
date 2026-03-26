use anyhow::Result;
use serde_json::{json, Value};

use crate::protocol::anthropic::{AnthropicContentBlock, AnthropicMessagesRequest, ToolChoice};
use crate::protocol::openai::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiFunctionCall, OpenAiToolCall, OpenAiToolChoice,
    OpenAiToolDefinition, OpenAiToolFunction,
};

const DEFAULT_MODEL: &str = "gpt-4o";
const HAIKU_MODEL: &str = "gpt-4o-mini";

pub fn map_model_name(model: &str) -> &'static str {
    let normalized = model.trim().to_ascii_lowercase();

    if normalized == "haiku" || normalized.contains("haiku") {
        HAIKU_MODEL
    } else if normalized == "sonnet"
        || normalized == "opus"
        || normalized.contains("sonnet")
        || normalized.contains("opus")
    {
        DEFAULT_MODEL
    } else {
        DEFAULT_MODEL
    }
}

pub fn map_anthropic_to_openai(request: &AnthropicMessagesRequest) -> Result<OpenAiChatRequest> {
    let mut messages = Vec::new();

    if let Some(system) = &request.system {
        messages.push(OpenAiChatMessage {
            role: "system".to_string(),
            content: Some(system.clone()),
            tool_call_id: None,
            tool_calls: vec![],
        });
    }

    for message in &request.messages {
        let mut text_blocks = Vec::new();
        let mut tool_calls = Vec::new();

        for block in &message.content {
            match block {
                AnthropicContentBlock::Text { text } => text_blocks.push(text.clone()),
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(OpenAiToolCall {
                        id: id.clone(),
                        kind: "function".to_string(),
                        function: OpenAiFunctionCall {
                            name: name.clone(),
                            arguments: serde_json::to_string(input)?,
                        },
                    })
                }
                AnthropicContentBlock::ToolResult {
                    tool_use_id,
                    content,
                } => {
                    push_role_message(
                        &mut messages,
                        &message.role,
                        &mut text_blocks,
                        &mut tool_calls,
                    );
                    messages.push(OpenAiChatMessage {
                        role: "tool".to_string(),
                        content: Some(content.clone()),
                        tool_call_id: Some(tool_use_id.clone()),
                        tool_calls: vec![],
                    })
                }
            }
        }

        push_role_message(
            &mut messages,
            &message.role,
            &mut text_blocks,
            &mut tool_calls,
        );
    }

    let tools = request
        .tools
        .iter()
        .map(|tool| OpenAiToolDefinition {
            kind: "function".to_string(),
            function: OpenAiToolFunction {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
            },
        })
        .collect();

    Ok(OpenAiChatRequest {
        model: map_model_name(&request.model).to_string(),
        messages,
        tools,
        tool_choice: request.tool_choice.as_ref().map(map_tool_choice),
        stream: request.stream,
        max_tokens: request.max_tokens,
    })
}

pub fn map_openai_to_anthropic_response(model: &str, response: &Value) -> Result<Value> {
    let choice = response
        .pointer("/choices/0")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            content.push(json!({
                "type": "text",
                "text": text,
            }));
        }
    }

    let tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for tool_call in &tool_calls {
        let input = map_tool_call_input(tool_call);
        content.push(json!({
            "type": "tool_use",
            "id": tool_call.get("id").and_then(Value::as_str).unwrap_or_default(),
            "name": tool_call.pointer("/function/name").and_then(Value::as_str).unwrap_or_default(),
            "input": input,
        }));
    }

    Ok(json!({
        "id": response.get("id").and_then(Value::as_str).unwrap_or("msg_codex_proxy"),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": map_stop_reason(choice.get("finish_reason").and_then(Value::as_str), !tool_calls.is_empty())
    }))
}

pub(crate) fn map_stop_reason(finish_reason: Option<&str>, has_tool_calls: bool) -> &'static str {
    match finish_reason {
        Some("length") => "max_tokens",
        Some("tool_calls") | Some("function_call") => "tool_use",
        Some("content_filter") => "refusal",
        Some("stop") => {
            if has_tool_calls {
                "tool_use"
            } else {
                "end_turn"
            }
        }
        Some(_) | None => {
            if has_tool_calls {
                "tool_use"
            } else {
                "end_turn"
            }
        }
    }
}

fn map_tool_call_input(tool_call: &Value) -> Value {
    match tool_call.pointer("/function/arguments") {
        Some(Value::Object(map)) => Value::Object(map.clone()),
        Some(Value::String(raw)) => map_tool_call_input_string(
            tool_call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            raw,
        ),
        Some(other) => {
            let tool_call_id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            tracing::warn!(
                tool_call_id,
                "tool arguments were not a string or object; wrapping value"
            );
            json!({ "__value": other.clone() })
        }
        None => Value::Object(Default::default()),
    }
}

fn map_tool_call_input_string(tool_call_id: &str, raw: &str) -> Value {
    if raw.trim().is_empty() {
        return Value::Object(Default::default());
    }

    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(other) => {
            tracing::warn!(
                tool_call_id,
                "tool arguments were valid JSON but not an object; wrapping value"
            );
            json!({ "__value": other })
        }
        Err(error) => {
            tracing::warn!(
                tool_call_id,
                %error,
                "tool arguments were malformed JSON; preserving raw payload"
            );
            json!({ "__raw_arguments": raw })
        }
    }
}

fn map_tool_choice(choice: &ToolChoice) -> OpenAiToolChoice {
    match choice {
        ToolChoice::Auto => OpenAiToolChoice::Auto,
        ToolChoice::Any => OpenAiToolChoice::Required,
    }
}

fn push_role_message(
    messages: &mut Vec<OpenAiChatMessage>,
    role: &str,
    text_blocks: &mut Vec<String>,
    tool_calls: &mut Vec<OpenAiToolCall>,
) {
    if text_blocks.is_empty() && tool_calls.is_empty() {
        return;
    }

    messages.push(OpenAiChatMessage {
        role: role.to_string(),
        content: (!text_blocks.is_empty()).then(|| text_blocks.join("\n\n")),
        tool_call_id: None,
        tool_calls: std::mem::take(tool_calls),
    });
    text_blocks.clear();
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{map_anthropic_to_openai, map_model_name, map_openai_to_anthropic_response};
    use crate::protocol::anthropic::{
        AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, ToolChoice,
    };
    use crate::protocol::openai::OpenAiToolChoice;

    #[test]
    fn maps_system_and_user_text_to_chat_completions_messages() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-5-sonnet-latest".to_string(),
            system: Some("You are concise.".to_string()),
            max_tokens: Some(512),
            stream: false,
            tools: vec![],
            tool_choice: None,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContentBlock::Text {
                    text: "Say hello".to_string(),
                }],
            }],
        };

        let mapped = map_anthropic_to_openai(&request).expect("mapping should work");
        assert_eq!(mapped.model, "gpt-4o");
        assert_eq!(mapped.messages[0].role, "system");
        assert_eq!(mapped.messages[1].role, "user");
    }

    #[test]
    fn coalesces_multiple_text_blocks_from_one_message() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-5-sonnet-latest".to_string(),
            system: None,
            max_tokens: Some(128),
            stream: false,
            tools: vec![],
            tool_choice: None,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![
                    AnthropicContentBlock::Text {
                        text: "First block".to_string(),
                    },
                    AnthropicContentBlock::Text {
                        text: "Second block".to_string(),
                    },
                ],
            }],
        };

        let mapped = map_anthropic_to_openai(&request).expect("mapping should work");
        assert_eq!(mapped.messages.len(), 1);
        assert_eq!(mapped.messages[0].role, "user");
        assert_eq!(
            mapped.messages[0].content.as_deref(),
            Some("First block\n\nSecond block")
        );
    }

    #[test]
    fn preserves_assistant_text_and_tool_use_in_a_single_turn() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-5-sonnet-latest".to_string(),
            system: None,
            max_tokens: Some(256),
            stream: false,
            tools: vec![],
            tool_choice: None,
            messages: vec![AnthropicMessage {
                role: "assistant".to_string(),
                content: vec![
                    AnthropicContentBlock::Text {
                        text: "I will look that up.".to_string(),
                    },
                    AnthropicContentBlock::ToolUse {
                        id: "toolu_lookup".to_string(),
                        name: "lookup".to_string(),
                        input: json!({"city":"Madrid"}),
                    },
                ],
            }],
        };

        let mapped = map_anthropic_to_openai(&request).expect("mapping should work");
        assert_eq!(mapped.messages.len(), 1);
        assert_eq!(mapped.messages[0].role, "assistant");
        assert_eq!(
            mapped.messages[0].content.as_deref(),
            Some("I will look that up.")
        );
        assert_eq!(mapped.messages[0].tool_calls.len(), 1);
        assert_eq!(mapped.messages[0].tool_calls[0].id, "toolu_lookup");
    }

    #[test]
    fn maps_tool_result_blocks_to_tool_messages() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-5-haiku-latest".to_string(),
            system: None,
            max_tokens: Some(256),
            stream: false,
            tools: vec![],
            tool_choice: Some(ToolChoice::Auto),
            messages: vec![
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: vec![AnthropicContentBlock::ToolUse {
                        id: "toolu_123".to_string(),
                        name: "lookup".to_string(),
                        input: json!({"city":"Madrid"}),
                    }],
                },
                AnthropicMessage {
                    role: "user".to_string(),
                    content: vec![AnthropicContentBlock::ToolResult {
                        tool_use_id: "toolu_123".to_string(),
                        content: "sunny".to_string(),
                    }],
                },
            ],
        };

        let mapped = map_anthropic_to_openai(&request).expect("mapping should work");
        assert_eq!(mapped.model, "gpt-4o-mini");
        assert_eq!(mapped.messages.last().expect("last").role, "tool");
    }

    #[test]
    fn maps_tool_choice_any_to_required() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-5-haiku-latest".to_string(),
            system: None,
            max_tokens: Some(64),
            stream: false,
            tools: vec![],
            tool_choice: Some(ToolChoice::Any),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContentBlock::Text {
                    text: "Use any tool you need".to_string(),
                }],
            }],
        };

        let mapped = map_anthropic_to_openai(&request).expect("mapping should work");
        assert_eq!(mapped.tool_choice, Some(OpenAiToolChoice::Required));
    }

    #[test]
    fn falls_back_to_default_model_for_unknown_claude_alias() {
        assert_eq!(map_model_name("claude-unknown"), "gpt-4o");
    }

    #[test]
    fn maps_current_claude_aliases_and_snapshots_to_openai_models() {
        assert_eq!(map_model_name("sonnet"), "gpt-4o");
        assert_eq!(map_model_name("opus"), "gpt-4o");
        assert_eq!(map_model_name("haiku"), "gpt-4o-mini");
        assert_eq!(map_model_name("claude-sonnet-4-20250514"), "gpt-4o");
        assert_eq!(map_model_name("claude-opus-4-1-20250805"), "gpt-4o");
        assert_eq!(map_model_name("claude-3-5-haiku-20241022"), "gpt-4o-mini");
    }

    #[test]
    fn maps_openai_tool_calls_to_anthropic_tool_use_blocks() {
        let response = json!({
            "id": "chatcmpl_tool",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "I will use a tool",
                    "tool_calls": [{
                        "id": "call_lookup_weather",
                        "type": "function",
                        "function": {
                            "name": "lookup_weather",
                            "arguments": "{\"city\":\"Madrid\"}"
                        }
                    }]
                }
            }]
        });

        let mapped = map_openai_to_anthropic_response("claude-3-5-sonnet-latest", &response)
            .expect("response mapping should work");
        assert_eq!(mapped["id"], "chatcmpl_tool");
        assert_eq!(mapped["stop_reason"], "tool_use");
        assert_eq!(mapped["content"][0]["type"], "text");
        assert_eq!(mapped["content"][0]["text"], "I will use a tool");
        assert_eq!(mapped["content"][1]["type"], "tool_use");
        assert_eq!(mapped["content"][1]["id"], "call_lookup_weather");
        assert_eq!(mapped["content"][1]["name"], "lookup_weather");
        assert_eq!(mapped["content"][1]["input"]["city"], "Madrid");
    }

    #[test]
    fn maps_openai_length_finish_reason_to_anthropic_max_tokens() {
        let response = json!({
            "id": "chatcmpl_tool",
            "choices": [{
                "finish_reason": "length",
                "message": {
                    "role": "assistant",
                    "content": "Partial tool call",
                    "tool_calls": [{
                        "id": "call_lookup_weather",
                        "type": "function",
                        "function": {
                            "name": "lookup_weather",
                            "arguments": "{\"city\":\"Mad"
                        }
                    }]
                }
            }]
        });

        let mapped = map_openai_to_anthropic_response("claude-3-5-sonnet-latest", &response)
            .expect("response mapping should work");
        assert_eq!(mapped["stop_reason"], "max_tokens");
    }

    #[test]
    fn preserves_malformed_tool_arguments_without_failing() {
        let response = json!({
            "id": "chatcmpl_tool",
            "choices": [{
                "finish_reason": "length",
                "message": {
                    "role": "assistant",
                    "content": "Partial tool call",
                    "tool_calls": [{
                        "id": "call_lookup_weather",
                        "type": "function",
                        "function": {
                            "name": "lookup_weather",
                            "arguments": "{\"city\":\"Mad"
                        }
                    }]
                }
            }]
        });

        let mapped = map_openai_to_anthropic_response("claude-3-5-sonnet-latest", &response)
            .expect("response mapping should work");
        assert_eq!(
            mapped["content"][1]["input"]["__raw_arguments"],
            "{\"city\":\"Mad"
        );
    }

    #[test]
    fn wraps_non_object_tool_arguments_in_an_object() {
        let response = json!({
            "id": "chatcmpl_tool",
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": "Tool call with array args",
                    "tool_calls": [{
                        "id": "call_lookup_weather",
                        "type": "function",
                        "function": {
                            "name": "lookup_weather",
                            "arguments": "[\"Madrid\",\"ES\"]"
                        }
                    }]
                }
            }]
        });

        let mapped = map_openai_to_anthropic_response("claude-3-5-sonnet-latest", &response)
            .expect("response mapping should work");
        assert_eq!(
            mapped["content"][1]["input"]["__value"],
            json!(["Madrid", "ES"])
        );
    }
}
