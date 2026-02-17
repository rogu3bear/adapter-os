//! Chat template processing for MLX inference
//!
//! Handles prompt formatting for different model architectures.
//! Each [`ModelFamily`] maps to a specific chat template format
//! (Llama 3, Llama 2, Qwen/ChatML, Mistral, or generic passthrough).

use serde::{Deserialize, Serialize};

/// Model family determines the chat template format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    /// `<|begin_of_text|><|start_header_id|>role<|end_header_id|>\n\ncontent<|eot_id|>`
    Llama3,
    /// `<<SYS>>\n...\n<</SYS>>\n\n[INST] ... [/INST]`
    Llama2,
    /// `<|im_start|>role\ncontent<|im_end|>` (ChatML)
    Qwen,
    /// `[INST] ... [/INST]`
    Mistral,
    /// Raw text passthrough — no special tokens
    Generic,
}

impl ModelFamily {
    /// Detect model family from an architecture string (e.g. from HuggingFace `config.json`).
    ///
    /// For Llama models, defaults to [`Llama3`](ModelFamily::Llama3). Use
    /// [`detect_with_vocab`](ModelFamily::detect_with_vocab) to disambiguate Llama 2 vs 3.
    pub fn detect(architecture: &str) -> Self {
        let arch = architecture.to_lowercase();
        if arch.contains("llama") {
            // Default to Llama3 when vocab size is unknown
            ModelFamily::Llama3
        } else if arch.contains("qwen") {
            ModelFamily::Qwen
        } else if arch.contains("mistral") {
            ModelFamily::Mistral
        } else {
            ModelFamily::Generic
        }
    }

    /// Detect model family with vocab size to disambiguate Llama 2 vs Llama 3.
    ///
    /// Llama 3 uses a large vocabulary (>= 128 000 tokens); Llama 2 has a smaller one.
    pub fn detect_with_vocab(architecture: &str, vocab_size: usize) -> Self {
        let arch = architecture.to_lowercase();
        if arch.contains("llama") {
            if vocab_size >= 128_000 {
                ModelFamily::Llama3
            } else {
                ModelFamily::Llama2
            }
        } else if arch.contains("qwen") {
            ModelFamily::Qwen
        } else if arch.contains("mistral") {
            ModelFamily::Mistral
        } else {
            ModelFamily::Generic
        }
    }
}

/// Chat message with role and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// Model-family-aware chat template engine.
///
/// Formats a `&[Message]` into the prompt string expected by the target model.
pub struct ChatTemplateEngine {
    family: ModelFamily,
}

impl ChatTemplateEngine {
    /// Create a template engine for the given model family.
    pub fn new(family: ModelFamily) -> Self {
        Self { family }
    }

    /// Create a template engine by detecting the family from an architecture string and vocab size.
    pub fn from_architecture(arch: &str, vocab_size: usize) -> Self {
        Self {
            family: ModelFamily::detect_with_vocab(arch, vocab_size),
        }
    }

    /// Apply the chat template to structured messages, returning a prompt string.
    pub fn apply(&self, messages: &[Message]) -> String {
        match self.family {
            ModelFamily::Llama3 => apply_llama3(messages),
            ModelFamily::Llama2 => apply_llama2(messages),
            ModelFamily::Qwen => apply_qwen(messages),
            ModelFamily::Mistral => apply_mistral(messages),
            ModelFamily::Generic => apply_generic(messages),
        }
    }

    /// Get the model family.
    pub fn family(&self) -> ModelFamily {
        self.family
    }
}

// ---------------------------------------------------------------------------
// Template implementations
// ---------------------------------------------------------------------------

fn apply_llama3(messages: &[Message]) -> String {
    let mut out = String::from("<|begin_of_text|>");
    for msg in messages {
        out.push_str("<|start_header_id|>");
        out.push_str(&msg.role);
        out.push_str("<|end_header_id|>\n\n");
        out.push_str(&msg.content);
        out.push_str("<|eot_id|>");
    }
    // Prompt for assistant generation
    out.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
    out
}

fn apply_llama2(messages: &[Message]) -> String {
    let mut out = String::from("<s>");
    let mut system_content: Option<&str> = None;
    let mut first_user = true;

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                system_content = Some(&msg.content);
            }
            "user" => {
                if first_user {
                    out.push_str("[INST] ");
                    if let Some(sys) = system_content.take() {
                        out.push_str("<<SYS>>\n");
                        out.push_str(sys);
                        out.push_str("\n<</SYS>>\n\n");
                    }
                    out.push_str(&msg.content);
                    out.push_str(" [/INST]");
                    first_user = false;
                } else {
                    out.push_str("[INST] ");
                    out.push_str(&msg.content);
                    out.push_str(" [/INST]");
                }
            }
            "assistant" => {
                out.push(' ');
                out.push_str(&msg.content);
                out.push_str(" </s>");
            }
            _ => {
                // Unknown role — treat as user content
                out.push_str(&msg.content);
            }
        }
    }
    out
}

fn apply_qwen(messages: &[Message]) -> String {
    let mut out = String::new();
    for msg in messages {
        out.push_str("<|im_start|>");
        out.push_str(&msg.role);
        out.push('\n');
        out.push_str(&msg.content);
        out.push_str("<|im_end|>\n");
    }
    // Prompt for assistant generation
    out.push_str("<|im_start|>assistant\n");
    out
}

fn apply_mistral(messages: &[Message]) -> String {
    let mut out = String::from("<s>");
    let mut system_content: Option<&str> = None;
    let mut first_user = true;

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                system_content = Some(&msg.content);
            }
            "user" => {
                out.push_str("[INST] ");
                if first_user {
                    if let Some(sys) = system_content.take() {
                        out.push_str(sys);
                        out.push_str("\n\n");
                    }
                    first_user = false;
                }
                out.push_str(&msg.content);
                out.push_str(" [/INST]");
            }
            "assistant" => {
                out.push(' ');
                out.push_str(&msg.content);
                out.push_str("</s>");
            }
            _ => {
                out.push_str(&msg.content);
            }
        }
    }
    out
}

fn apply_generic(messages: &[Message]) -> String {
    let mut out = String::new();
    for msg in messages {
        out.push_str(&msg.content);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llama3_template() {
        let engine = ChatTemplateEngine::new(ModelFamily::Llama3);
        let messages = vec![Message::system("You are helpful."), Message::user("Hi")];
        let result = engine.apply(&messages);
        assert!(result.starts_with("<|begin_of_text|>"));
        assert!(result
            .contains("<|start_header_id|>system<|end_header_id|>\n\nYou are helpful.<|eot_id|>"));
        assert!(result.contains("<|start_header_id|>user<|end_header_id|>\n\nHi<|eot_id|>"));
        assert!(result.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn test_qwen_chatml_template() {
        let engine = ChatTemplateEngine::new(ModelFamily::Qwen);
        let messages = vec![Message::system("You are helpful."), Message::user("Hi")];
        let result = engine.apply(&messages);
        assert!(result.contains("<|im_start|>system\nYou are helpful.<|im_end|>"));
        assert!(result.contains("<|im_start|>user\nHi<|im_end|>"));
        assert!(result.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn test_mistral_template() {
        let engine = ChatTemplateEngine::new(ModelFamily::Mistral);
        let messages = vec![Message::user("Hi")];
        let result = engine.apply(&messages);
        assert!(result.starts_with("<s>"));
        assert!(result.contains("[INST] Hi [/INST]"));
    }

    #[test]
    fn test_llama2_template() {
        let engine = ChatTemplateEngine::new(ModelFamily::Llama2);
        let messages = vec![Message::system("Be concise."), Message::user("Hi")];
        let result = engine.apply(&messages);
        assert!(result.starts_with("<s>"));
        assert!(result.contains("<<SYS>>\nBe concise.\n<</SYS>>"));
        assert!(result.contains("[INST] "));
        assert!(result.contains("Hi [/INST]"));
    }

    #[test]
    fn test_generic_passthrough() {
        let engine = ChatTemplateEngine::new(ModelFamily::Generic);
        let messages = vec![Message::system("System prompt"), Message::user("Hello")];
        let result = engine.apply(&messages);
        assert_eq!(result, "System prompt\nHello\n");
        // No special tokens
        assert!(!result.contains("<|"));
        assert!(!result.contains("[INST]"));
    }

    #[test]
    fn test_model_family_detection() {
        assert_eq!(ModelFamily::detect("llama"), ModelFamily::Llama3);
        assert_eq!(ModelFamily::detect("LlamaForCausalLM"), ModelFamily::Llama3);
        assert_eq!(ModelFamily::detect("qwen2"), ModelFamily::Qwen);
        assert_eq!(ModelFamily::detect("Qwen2.5"), ModelFamily::Qwen);
        assert_eq!(ModelFamily::detect("mistral"), ModelFamily::Mistral);
        assert_eq!(
            ModelFamily::detect("MistralForCausalLM"),
            ModelFamily::Mistral
        );
        assert_eq!(ModelFamily::detect("gpt-neox"), ModelFamily::Generic);
    }

    #[test]
    fn test_model_family_detection_with_vocab() {
        // Llama 3 has large vocab
        assert_eq!(
            ModelFamily::detect_with_vocab("llama", 128_256),
            ModelFamily::Llama3
        );
        // Llama 2 has small vocab
        assert_eq!(
            ModelFamily::detect_with_vocab("llama", 32_000),
            ModelFamily::Llama2
        );
        // Non-llama ignores vocab_size
        assert_eq!(
            ModelFamily::detect_with_vocab("qwen2", 151_936),
            ModelFamily::Qwen
        );
    }

    #[test]
    fn test_multi_turn_conversation() {
        let engine = ChatTemplateEngine::new(ModelFamily::Llama3);
        let messages = vec![
            Message::system("You are helpful."),
            Message::user("What is 2+2?"),
            Message::assistant("4"),
            Message::user("And 3+3?"),
        ];
        let result = engine.apply(&messages);

        // All messages present
        assert!(result.contains("What is 2+2?"));
        assert!(result.contains("4"));
        assert!(result.contains("And 3+3?"));

        // Verify structure: 4 message blocks + trailing assistant prompt
        let header_count = result.matches("<|start_header_id|>").count();
        // 4 messages + 1 trailing assistant prompt = 5
        assert_eq!(header_count, 5);
    }

    #[test]
    fn test_multi_turn_qwen() {
        let engine = ChatTemplateEngine::new(ModelFamily::Qwen);
        let messages = vec![
            Message::system("You are helpful."),
            Message::user("What is 2+2?"),
            Message::assistant("4"),
            Message::user("And 3+3?"),
        ];
        let result = engine.apply(&messages);
        let im_start_count = result.matches("<|im_start|>").count();
        // 4 messages + 1 trailing assistant prompt = 5
        assert_eq!(im_start_count, 5);
    }

    #[test]
    fn test_multi_turn_llama2() {
        let engine = ChatTemplateEngine::new(ModelFamily::Llama2);
        let messages = vec![
            Message::system("Be helpful."),
            Message::user("Hi"),
            Message::assistant("Hello!"),
            Message::user("Bye"),
        ];
        let result = engine.apply(&messages);
        assert!(result.contains("<<SYS>>\nBe helpful.\n<</SYS>>"));
        assert!(result.contains("Hi [/INST]"));
        assert!(result.contains("Hello! </s>"));
        assert!(result.contains("[INST] Bye [/INST]"));
    }

    #[test]
    fn test_system_message_handling_mistral() {
        // Mistral prepends system content to first user message
        let engine = ChatTemplateEngine::new(ModelFamily::Mistral);
        let messages = vec![Message::system("Be concise."), Message::user("Hi")];
        let result = engine.apply(&messages);
        assert!(result.contains("[INST] Be concise.\n\nHi [/INST]"));
    }

    #[test]
    fn test_system_message_handling_no_system() {
        // When there's no system message, templates should still work
        for family in [
            ModelFamily::Llama3,
            ModelFamily::Llama2,
            ModelFamily::Qwen,
            ModelFamily::Mistral,
            ModelFamily::Generic,
        ] {
            let engine = ChatTemplateEngine::new(family);
            let messages = vec![Message::user("Hello")];
            let result = engine.apply(&messages);
            assert!(
                result.contains("Hello"),
                "{family:?} template should contain user content"
            );
        }
    }

    #[test]
    fn test_from_architecture() {
        let engine = ChatTemplateEngine::from_architecture("llama", 128_256);
        assert_eq!(engine.family(), ModelFamily::Llama3);

        let engine = ChatTemplateEngine::from_architecture("llama", 32_000);
        assert_eq!(engine.family(), ModelFamily::Llama2);

        let engine = ChatTemplateEngine::from_architecture("qwen2", 151_936);
        assert_eq!(engine.family(), ModelFamily::Qwen);
    }

    #[test]
    fn test_serde_round_trip() {
        let family = ModelFamily::Llama3;
        let json = serde_json::to_string(&family).expect("serialize");
        assert_eq!(json, "\"llama3\"");
        let back: ModelFamily = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, family);

        let family = ModelFamily::Qwen;
        let json = serde_json::to_string(&family).expect("serialize");
        assert_eq!(json, "\"qwen\"");
    }
}
