use anyhow::Result;

use crate::protocol::anthropic::{AnthropicContentBlock, AnthropicMessagesRequest, ToolChoice};
use crate::protocol::openai::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiFunctionCall, OpenAiToolCall, OpenAiToolChoice,
    OpenAiToolDefinition, OpenAiToolFunction,
};

const DEFAULT_MODEL: &str = "gpt-4o";
const HAIKU_MODEL: &str = "gpt-4o-mini";

pub fn map_model_name(model: &str) -> &'static str {
    if model.starts_with("claude-3-5-haiku-") {
        HAIKU_MODEL
    } else if model.starts_with("claude-3-5-sonnet-") {
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

    use super::{map_anthropic_to_openai, map_model_name};
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
}
