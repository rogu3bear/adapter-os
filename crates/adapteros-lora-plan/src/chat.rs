//! Chat template processing for MLX inference
//!
//! Handles prompt formatting for different model architectures

use adapteros_core::Result;
use serde::{Deserialize, Serialize};

/// Special tokens for chat templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialTokens {
    pub bos: String,
    pub eos: String,
    pub unk: String,
    pub pad: String,
}

impl Default for SpecialTokens {
    fn default() -> Self {
        Self {
            bos: "<|im_start|>".to_string(),
            eos: "<|im_end|>".to_string(),
            unk: "<|unk|>".to_string(),
            pad: "<|pad|>".to_string(),
        }
    }
}

/// Chat template configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTemplate {
    pub name: String,
    pub template: String,
    pub special_tokens: SpecialTokens,
}

impl Default for ChatTemplate {
    fn default() -> Self {
        Self {
            name: "chatml".to_string(),
            template: "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}".to_string(),
            special_tokens: SpecialTokens::default(),
        }
    }
}

/// Chat template processor
pub struct ChatTemplateProcessor {
    template: ChatTemplate,
}

impl ChatTemplateProcessor {
    /// Create a new chat template processor
    pub fn new(template: ChatTemplate) -> Self {
        Self { template }
    }

    /// Apply template to messages
    pub fn apply(&self, messages: &[Message]) -> Result<String> {
        // Simple implementation for Qwen/ChatML format
        let mut result = String::new();

        for message in messages {
            result.push_str(&self.template.special_tokens.bos);
            result.push_str(&message.role);
            result.push('\n');
            result.push_str(&message.content);
            result.push_str(&self.template.special_tokens.eos);
            result.push('\n');
        }

        Ok(result)
    }

    /// Get special tokens
    pub fn special_tokens(&self) -> &SpecialTokens {
        &self.template.special_tokens
    }
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_template() {
        let processor = ChatTemplateProcessor::new(ChatTemplate::default());
        let messages = vec![
            Message::system("You are a helpful assistant."),
            Message::user("Hello!"),
        ];

        let result = processor
            .apply(&messages)
            .expect("Operation should succeed");
        assert!(result.contains("<|im_start|>system"));
        assert!(result.contains("<|im_start|>user"));
        assert!(result.contains("<|im_end|>"));
    }
}
