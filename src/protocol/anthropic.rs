use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    #[serde(default)]
    pub system: Option<AnthropicSystemPrompt>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub tools: Vec<AnthropicToolDefinition>,
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,
    pub messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AnthropicSystemPrompt {
    Text(String),
    Blocks(Vec<AnthropicSystemBlock>),
}

impl AnthropicSystemPrompt {
    pub fn into_text(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Blocks(blocks) => blocks
                .into_iter()
                .map(|block| match block {
                    AnthropicSystemBlock::Text { text } => text,
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        }
    }

    pub fn text(&self) -> String {
        self.clone().into_text()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum AnthropicSystemBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnthropicToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Any,
}
