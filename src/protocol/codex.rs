use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::protocol::openai::{OpenAiChatMessage, OpenAiChatRequest, OpenAiToolChoice};

const DEFAULT_CODEX_PROMPT_BUDGET: usize = 12_000;
const DEFAULT_PRESERVED_RECENT_MESSAGES: usize = 8;
const DEFAULT_OLDER_TEXT_CHAR_LIMIT: usize = 1_200;
const DEFAULT_OLDER_TOOL_RESULT_CHAR_LIMIT: usize = 600;
const TEXT_TRUNCATION_MARKER: &str = "...[truncated by claude-codex]";
const TOOL_RESULT_TRUNCATION_MARKER: &str = "...[tool result truncated by claude-codex]";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexEffortLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexResponsesRequest {
    pub model: String,
    pub store: bool,
    pub stream: bool,
    pub instructions: String,
    pub input: Vec<CodexInputMessage>,
    pub reasoning: CodexReasoningOptions,
    pub text: CodexTextOptions,
    pub include: Vec<String>,
    pub tool_choice: String,
    pub parallel_tool_calls: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<CodexToolDecl>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexTextOptions {
    pub verbosity: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexReasoningOptions {
    pub effort: CodexEffortLevel,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexInputMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexToolDecl {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub parameters: Value,
}

pub fn build_codex_request(
    request: &OpenAiChatRequest,
    effort: CodexEffortLevel,
) -> CodexResponsesRequest {
    let optimized = optimize_codex_request(request);
    let mut instructions = Vec::new();
    let mut input = Vec::new();

    for message in &optimized.messages {
        match message.role.as_str() {
            "system" => {
                if let Some(content) = message
                    .content
                    .as_deref()
                    .filter(|content| !content.is_empty())
                {
                    instructions.push(content.to_string());
                }
            }
            "user" | "assistant" => {
                if let Some(content) = message
                    .content
                    .as_deref()
                    .filter(|content| !content.is_empty())
                {
                    input.push(CodexInputMessage {
                        role: message.role.clone(),
                        content: content.to_string(),
                    });
                }

                for tool_call in &message.tool_calls {
                    input.push(CodexInputMessage {
                        role: "assistant".to_string(),
                        content: format!(
                            "[called tool: {} with {}]",
                            tool_call.function.name, tool_call.function.arguments
                        ),
                    });
                }
            }
            "tool" => input.push(CodexInputMessage {
                role: "user".to_string(),
                content: summarize_tool_result(message),
            }),
            _ => {
                if let Some(content) = message
                    .content
                    .as_deref()
                    .filter(|content| !content.is_empty())
                {
                    input.push(CodexInputMessage {
                        role: message.role.clone(),
                        content: content.to_string(),
                    });
                }
            }
        }
    }

    let tools = optimized
        .tools
        .iter()
        .map(|tool| CodexToolDecl {
            kind: "function".to_string(),
            name: tool.function.name.clone(),
            description: tool.function.description.clone(),
            parameters: tool.function.parameters.clone(),
        })
        .collect();

    CodexResponsesRequest {
        model: optimized.model.clone(),
        store: false,
        stream: optimized.stream,
        instructions: instructions.join("\n\n"),
        input,
        reasoning: CodexReasoningOptions { effort },
        text: CodexTextOptions {
            verbosity: "low".to_string(),
        },
        include: vec!["reasoning.encrypted_content".to_string()],
        tool_choice: map_tool_choice(optimized.tool_choice.as_ref()),
        parallel_tool_calls: true,
        tools,
    }
}

fn optimize_codex_request(request: &OpenAiChatRequest) -> OpenAiChatRequest {
    let mut optimized = request.clone();
    let system_indices = optimized
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.role == "system").then_some(index))
        .collect::<Vec<_>>();
    let non_system_indices = optimized
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.role != "system").then_some(index))
        .collect::<Vec<_>>();

    if non_system_indices.len() <= DEFAULT_PRESERVED_RECENT_MESSAGES {
        return optimized;
    }

    let preserved_start = non_system_indices.len() - DEFAULT_PRESERVED_RECENT_MESSAGES;
    let trimmable_indices = &non_system_indices[..preserved_start];

    for &index in trimmable_indices {
        trim_message(&mut optimized.messages[index]);
    }

    if estimate_codex_prompt_tokens(&optimized) <= DEFAULT_CODEX_PROMPT_BUDGET {
        return optimized;
    }

    let mut keep_flags = vec![true; optimized.messages.len()];
    for &index in &system_indices {
        keep_flags[index] = true;
    }

    for &index in trimmable_indices {
        if estimate_codex_prompt_tokens_with_flags(&optimized, &keep_flags)
            <= DEFAULT_CODEX_PROMPT_BUDGET
        {
            break;
        }
        keep_flags[index] = false;
    }

    optimized.messages = optimized
        .messages
        .into_iter()
        .enumerate()
        .filter_map(|(index, message)| keep_flags[index].then_some(message))
        .collect();
    optimized
}

fn trim_message(message: &mut OpenAiChatMessage) {
    if let Some(content) = message.content.take() {
        let limit = if message.role == "tool" {
            DEFAULT_OLDER_TOOL_RESULT_CHAR_LIMIT
        } else {
            DEFAULT_OLDER_TEXT_CHAR_LIMIT
        };
        let marker = if message.role == "tool" {
            TOOL_RESULT_TRUNCATION_MARKER
        } else {
            TEXT_TRUNCATION_MARKER
        };
        message.content = Some(truncate_content(&content, limit, marker));
    }
}

fn truncate_content(content: &str, limit: usize, marker: &str) -> String {
    if content.chars().count() <= limit {
        return content.to_string();
    }

    let marker_len = marker.chars().count();
    if limit <= marker_len {
        return marker.chars().take(limit).collect();
    }

    let prefix: String = content.chars().take(limit - marker_len).collect();
    format!("{prefix}{marker}")
}

fn estimate_codex_prompt_tokens(request: &OpenAiChatRequest) -> usize {
    let keep_flags = vec![true; request.messages.len()];
    estimate_codex_prompt_tokens_with_flags(request, &keep_flags)
}

fn estimate_codex_prompt_tokens_with_flags(
    request: &OpenAiChatRequest,
    keep_flags: &[bool],
) -> usize {
    let message_tokens = request
        .messages
        .iter()
        .enumerate()
        .filter(|(index, _)| keep_flags.get(*index).copied().unwrap_or(true))
        .map(|(_, message)| estimate_message_tokens(message))
        .sum::<usize>();
    let tool_tokens = request
        .tools
        .iter()
        .map(|tool| serialized_token_estimate(tool))
        .sum::<usize>();

    message_tokens + tool_tokens
}

fn estimate_message_tokens(message: &OpenAiChatMessage) -> usize {
    let role_tokens = string_token_estimate(&message.role);
    let content_tokens = message
        .content
        .as_deref()
        .map(string_token_estimate)
        .unwrap_or(0);
    let tool_call_id_tokens = message
        .tool_call_id
        .as_deref()
        .map(string_token_estimate)
        .unwrap_or(0);
    let tool_call_tokens = message
        .tool_calls
        .iter()
        .map(serialized_token_estimate)
        .sum::<usize>();

    role_tokens + content_tokens + tool_call_id_tokens + tool_call_tokens
}

fn string_token_estimate(value: &str) -> usize {
    serialized_len_to_token_estimate(value.len())
}

fn serialized_token_estimate<T: Serialize>(value: &T) -> usize {
    let len = serde_json::to_string(value)
        .map(|json| json.len())
        .unwrap_or_default();
    serialized_len_to_token_estimate(len)
}

fn serialized_len_to_token_estimate(len: usize) -> usize {
    len.div_ceil(4)
}

fn summarize_tool_result(message: &OpenAiChatMessage) -> String {
    let content = message.content.as_deref().unwrap_or_default();
    let call_id = message.tool_call_id.as_deref().unwrap_or("unknown");
    format!("[tool result for call {}: {}]", call_id, content)
}

fn map_tool_choice(tool_choice: Option<&OpenAiToolChoice>) -> String {
    match tool_choice {
        Some(OpenAiToolChoice::Required) => "required".to_string(),
        Some(OpenAiToolChoice::Auto) | None => "auto".to_string(),
    }
}

#[derive(Debug, Default)]
pub struct CodexSseToOpenAiBridge {
    buffer: Vec<u8>,
    text_output: String,
    pending_tool_arguments: String,
    tool_calls: Vec<CodexCompletedToolCall>,
    emitted_finish: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexCompletedToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl CodexSseToOpenAiBridge {
    pub fn push_bytes(&mut self, chunk: &[u8]) -> Result<String> {
        self.buffer.extend_from_slice(chunk);
        let mut output = String::new();

        while let Some((frame_end, delimiter_len)) = find_frame_boundary(&self.buffer) {
            let frame_bytes = self.buffer[..frame_end].to_vec();
            self.buffer.drain(..frame_end + delimiter_len);
            let frame = String::from_utf8(frame_bytes)?;
            output.push_str(&self.translate_frame(frame.trim())?);
        }

        Ok(output)
    }

    pub fn finish_stream(&mut self) -> String {
        if self.emitted_finish {
            return String::new();
        }

        self.emitted_finish = true;
        let finish_reason = if self.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        format!(
            "data: {}\n\n\
             data: [DONE]\n\n",
            json!({
                "choices": [{
                    "finish_reason": finish_reason
                }]
            })
        )
    }

    pub fn into_chat_response(mut self) -> Value {
        let _ = self.finish_stream();
        json!({
            "id": "chatcmpl_codex_proxy",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": self.text_output,
                    "tool_calls": self.tool_calls.iter().map(|tool_call| json!({
                        "id": tool_call.id,
                        "type": "function",
                        "function": {
                            "name": tool_call.name,
                            "arguments": tool_call.arguments,
                        }
                    })).collect::<Vec<_>>()
                },
                "finish_reason": if self.tool_calls.is_empty() { "stop" } else { "tool_calls" }
            }]
        })
    }

    fn translate_frame(&mut self, frame: &str) -> Result<String> {
        let payload = extract_sse_payload(frame);
        if payload.is_empty() {
            return Ok(String::new());
        }

        if payload == "[DONE]" {
            return Ok(self.finish_stream());
        }

        let value: Value = serde_json::from_str(&payload)?;
        let event_type = extract_event_type(frame, &value);
        let mut output = String::new();

        match event_type.as_str() {
            "response.output_text.delta" => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    self.text_output.push_str(delta);
                    output.push_str(&format_openai_stream_delta(json!({
                        "content": delta
                    })));
                }
            }
            "response.function_call_arguments.delta" => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    self.pending_tool_arguments.push_str(delta);
                }
            }
            "response.output_item.done" => {
                if let Some(item) = value.get("item") {
                    if item.get("type").and_then(Value::as_str) == Some("function_call") {
                        let id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or("call_codex_proxy")
                            .to_string();
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string();
                        let arguments = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .filter(|arguments| !arguments.is_empty())
                            .unwrap_or_else(|| std::mem::take(&mut self.pending_tool_arguments));
                        let index = self.tool_calls.len();
                        self.tool_calls.push(CodexCompletedToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                        });
                        output.push_str(&format_openai_stream_delta(json!({
                            "tool_calls": [{
                                "index": index,
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": ""
                                }
                            }]
                        })));
                        if !arguments.is_empty() {
                            output.push_str(&format_openai_stream_delta(json!({
                                "tool_calls": [{
                                    "index": index,
                                    "function": {
                                        "arguments": arguments
                                    }
                                }]
                            })));
                        }
                    }
                }
            }
            "response.completed" => output.push_str(&self.finish_stream()),
            "response.failed" => {
                let message = value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("codex responses request failed");
                bail!(message.to_string());
            }
            _ => {}
        }

        Ok(output)
    }
}

fn extract_event_type(frame: &str, value: &Value) -> String {
    frame
        .lines()
        .find_map(|line| line.strip_prefix("event: ").map(str::to_string))
        .or_else(|| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn extract_sse_payload(frame: &str) -> String {
    frame
        .lines()
        .filter_map(|line| line.strip_prefix("data: ").map(str::trim))
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_frame_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    for delimiter in [b"\n\n".as_slice(), b"\r\n\r\n".as_slice()] {
        if let Some(index) = buffer
            .windows(delimiter.len())
            .position(|window| window == delimiter)
        {
            return Some((index, delimiter.len()));
        }
    }

    None
}

fn format_openai_stream_delta(delta: Value) -> String {
    format!(
        "data: {}\n\n",
        json!({
            "choices": [{
                "delta": delta
            }]
        })
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_codex_request, estimate_codex_prompt_tokens, optimize_codex_request,
        CodexEffortLevel, CodexSseToOpenAiBridge, DEFAULT_CODEX_PROMPT_BUDGET,
        DEFAULT_OLDER_TEXT_CHAR_LIMIT, DEFAULT_OLDER_TOOL_RESULT_CHAR_LIMIT,
        DEFAULT_PRESERVED_RECENT_MESSAGES, TEXT_TRUNCATION_MARKER, TOOL_RESULT_TRUNCATION_MARKER,
    };
    use crate::protocol::openai::{
        OpenAiChatMessage, OpenAiChatRequest, OpenAiFunctionCall, OpenAiToolCall, OpenAiToolChoice,
        OpenAiToolDefinition, OpenAiToolFunction,
    };

    #[test]
    fn builds_codex_requests_from_chat_completions_requests() {
        let request = OpenAiChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: Some("You are concise.".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: Some("Hello".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
            ],
            tools: vec![OpenAiToolDefinition {
                kind: "function".to_string(),
                function: OpenAiToolFunction {
                    name: "lookup_weather".to_string(),
                    description: Some("Lookup the weather".to_string()),
                    parameters: json!({"type":"object"}),
                },
            }],
            tool_choice: Some(OpenAiToolChoice::Auto),
            stream: false,
            max_tokens: Some(128),
        };

        let built = build_codex_request(&request, CodexEffortLevel::Medium);
        assert_eq!(built.model, "gpt-4o");
        assert_eq!(built.instructions, "You are concise.");
        assert_eq!(built.input.len(), 1);
        assert_eq!(built.input[0].role, "user");
        assert_eq!(built.tools[0].name, "lookup_weather");
        assert_eq!(built.reasoning.effort, CodexEffortLevel::Medium);
        assert_eq!(built.text.verbosity, "low");
    }

    #[test]
    fn builds_codex_requests_with_low_effort() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![],
            tools: vec![],
            tool_choice: None,
            stream: false,
            max_tokens: None,
        };

        let built = build_codex_request(&request, CodexEffortLevel::Low);
        assert_eq!(built.reasoning.effort, CodexEffortLevel::Low);
    }

    #[test]
    fn builds_codex_requests_with_high_effort() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![],
            tools: vec![],
            tool_choice: None,
            stream: false,
            max_tokens: None,
        };

        let built = build_codex_request(&request, CodexEffortLevel::High);
        assert_eq!(built.reasoning.effort, CodexEffortLevel::High);
    }

    #[test]
    fn estimator_counts_system_messages_and_tools() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: Some("You are concise.".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: Some("Hello".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
            ],
            tools: vec![OpenAiToolDefinition {
                kind: "function".to_string(),
                function: OpenAiToolFunction {
                    name: "lookup_weather".to_string(),
                    description: Some("Lookup the weather".to_string()),
                    parameters: json!({"type":"object","properties":{"city":{"type":"string"}}}),
                },
            }],
            tool_choice: None,
            stream: true,
            max_tokens: None,
        };

        assert!(estimate_codex_prompt_tokens(&request) > 0);
    }

    #[test]
    fn preserves_newest_non_system_messages_unchanged() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: (0..10)
                .map(|index| OpenAiChatMessage {
                    role: "user".to_string(),
                    content: Some(format!(
                        "message-{index}-{}",
                        "x".repeat(DEFAULT_OLDER_TEXT_CHAR_LIMIT + 200)
                    )),
                    tool_call_id: None,
                    tool_calls: vec![],
                })
                .collect(),
            tools: vec![],
            tool_choice: None,
            stream: true,
            max_tokens: None,
        };

        let optimized = optimize_codex_request(&request);
        let original_recent =
            &request.messages[request.messages.len() - DEFAULT_PRESERVED_RECENT_MESSAGES..];
        let optimized_recent =
            &optimized.messages[optimized.messages.len() - DEFAULT_PRESERVED_RECENT_MESSAGES..];

        assert_eq!(optimized_recent, original_recent);
    }

    #[test]
    fn truncates_older_text_messages() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: (0..10)
                .map(|index| OpenAiChatMessage {
                    role: "user".to_string(),
                    content: Some(format!(
                        "message-{index}-{}",
                        "x".repeat(DEFAULT_OLDER_TEXT_CHAR_LIMIT + 200)
                    )),
                    tool_call_id: None,
                    tool_calls: vec![],
                })
                .collect(),
            tools: vec![],
            tool_choice: None,
            stream: true,
            max_tokens: None,
        };

        let optimized = optimize_codex_request(&request);
        let first = optimized.messages.first().expect("first message");
        let first_content = first.content.as_deref().expect("content");

        assert!(first_content.ends_with(TEXT_TRUNCATION_MARKER));
        assert_eq!(first_content.chars().count(), DEFAULT_OLDER_TEXT_CHAR_LIMIT);
    }

    #[test]
    fn truncates_older_tool_results() {
        let mut messages = Vec::new();
        messages.push(OpenAiChatMessage {
            role: "tool".to_string(),
            content: Some("y".repeat(DEFAULT_OLDER_TOOL_RESULT_CHAR_LIMIT + 200)),
            tool_call_id: Some("call_123".to_string()),
            tool_calls: vec![],
        });
        messages.extend((0..8).map(|index| OpenAiChatMessage {
            role: "user".to_string(),
            content: Some(format!("recent-{index}")),
            tool_call_id: None,
            tool_calls: vec![],
        }));

        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages,
            tools: vec![],
            tool_choice: None,
            stream: true,
            max_tokens: None,
        };

        let optimized = optimize_codex_request(&request);
        let first_content = optimized.messages[0]
            .content
            .as_deref()
            .expect("tool content");

        assert!(first_content.ends_with(TOOL_RESULT_TRUNCATION_MARKER));
        assert_eq!(
            first_content.chars().count(),
            DEFAULT_OLDER_TOOL_RESULT_CHAR_LIMIT
        );
    }

    #[test]
    fn drops_oldest_non_system_messages_when_trimming_is_not_enough() {
        let mut messages = Vec::new();
        messages.push(OpenAiChatMessage {
            role: "system".to_string(),
            content: Some("keep me".to_string()),
            tool_call_id: None,
            tool_calls: vec![],
        });
        messages.extend((0..20).map(|index| OpenAiChatMessage {
            role: "user".to_string(),
            content: Some(format!(
                "old-{index}-{}",
                "z".repeat(DEFAULT_CODEX_PROMPT_BUDGET * 8)
            )),
            tool_call_id: None,
            tool_calls: vec![],
        }));

        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages,
            tools: vec![],
            tool_choice: None,
            stream: true,
            max_tokens: None,
        };

        let optimized = optimize_codex_request(&request);

        assert_eq!(
            optimized.messages[0].content.as_deref(),
            Some("keep me"),
            "system prompt should be preserved"
        );
        assert!(
            optimized.messages.len() < request.messages.len(),
            "older messages should have been dropped"
        );
        assert!(
            optimized.messages.len() >= DEFAULT_PRESERVED_RECENT_MESSAGES + 1,
            "newest preserved messages and system prompt should remain"
        );
    }

    #[test]
    fn preserves_assistant_tool_calls_when_optimizing() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "assistant".to_string(),
                    content: Some("I will call a tool".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![OpenAiToolCall {
                        id: "call_1".to_string(),
                        kind: "function".to_string(),
                        function: OpenAiFunctionCall {
                            name: "lookup_weather".to_string(),
                            arguments: "{\"city\":\"Madrid\"}".to_string(),
                        },
                    }],
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: Some("recent".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
            ],
            tools: vec![],
            tool_choice: None,
            stream: true,
            max_tokens: None,
        };

        let optimized = optimize_codex_request(&request);
        assert_eq!(
            optimized.messages[0].tool_calls,
            request.messages[0].tool_calls
        );
    }

    #[test]
    fn translates_codex_text_deltas_into_openai_stream_chunks() {
        let mut bridge = CodexSseToOpenAiBridge::default();
        let output = bridge
            .push_bytes(
                b"event: response.output_text.delta\ndata: {\"delta\":\"Hello\"}\n\n\
                  event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n",
            )
            .expect("bridge should translate");

        assert!(output.contains("\"content\":\"Hello\""));
        assert!(output.contains("\"finish_reason\":\"stop\""));
        assert!(output.contains("data: [DONE]"));
    }

    #[test]
    fn accumulates_codex_tool_calls_into_chat_completions_shape() {
        let mut bridge = CodexSseToOpenAiBridge::default();
        bridge
            .push_bytes(
                b"event: response.function_call_arguments.delta\ndata: {\"delta\":\"{\\\"city\\\":\\\"Madrid\\\"}\"}\n\n\
                  event: response.output_item.done\ndata: {\"item\":{\"type\":\"function_call\",\"call_id\":\"call_weather\",\"name\":\"lookup_weather\"}}\n\n\
                  data: [DONE]\n\n",
            )
            .expect("bridge should accumulate tool calls");

        let response = bridge.into_chat_response();
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "lookup_weather"
        );
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"Madrid\"}"
        );
    }
}
