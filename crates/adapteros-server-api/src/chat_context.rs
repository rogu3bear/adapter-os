//! Multi-turn chat context builder for server-side prompt construction.
//!
//! This module provides deterministic prompt building from chat session history,
//! enabling server-side multi-turn conversations while preserving replay capability.
//!
//! # Determinism Guarantees
//!
//! The `build_chat_prompt` function is a pure function of stored data:
//! - Messages loaded via `get_chat_messages` are ordered by `timestamp ASC`
//! - Truncation rules are deterministic (fixed message count + token budget)
//! - No wall-clock or RNG dependencies
//! - Context hash computed from sorted message IDs for audit/replay verification
//!
//! # Format
//!
//! Prompts are formatted with simple `[role]:` prefixes:
//! ```text
//! [system]: You are adapterOS.
//! [user]: Hello
//! [assistant]: Hi there
//! [user]: New message
//! ```

use adapteros_db::chat_sessions::ChatMessage;
use adapteros_db::Db;
use blake3::Hasher;
use thiserror::Error;
use tracing::debug;

use crate::state::ChatContextConfig;

/// Token estimation: ~4 characters per token (heuristic)
const CHARS_PER_TOKEN: usize = 4;

/// Result of building a chat prompt from session history
#[derive(Debug, Clone)]
pub struct ChatPromptResult {
    /// The formatted multi-turn prompt text
    pub prompt_text: String,
    /// Number of history messages included (excluding the new message)
    pub message_count: usize,
    /// Whether history was truncated due to limits
    pub truncated: bool,
    /// BLAKE3 hash of message IDs for replay verification
    pub context_hash: String,
}

/// Errors that can occur when building chat prompts
#[derive(Error, Debug)]
pub enum ChatContextError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
}

/// Build a deterministic multi-turn prompt from session history.
///
/// This function loads chat history from the database, applies truncation rules,
/// and formats the result as a prompt with role markers.
///
/// # Arguments
///
/// * `db` - Database access for loading chat messages
/// * `session_id` - The chat session to load history from
/// * `new_user_message` - The new user message to append
/// * `config` - Chat context configuration (limits, etc.)
///
/// # Returns
///
/// A `ChatPromptResult` containing the formatted prompt and metadata.
///
/// # Determinism
///
/// This function is deterministic: given the same database state and inputs,
/// it will always produce the same output. This is critical for replay support.
pub async fn build_chat_prompt(
    db: &Db,
    session_id: &str,
    new_user_message: &str,
    config: &ChatContextConfig,
) -> Result<ChatPromptResult, ChatContextError> {
    // Load messages from database (already ordered by timestamp ASC)
    let messages = db
        .get_chat_messages(session_id, Some(config.max_history_messages as i64))
        .await
        .map_err(|e| ChatContextError::Database(e.to_string()))?;

    debug!(
        session_id = %session_id,
        loaded_messages = messages.len(),
        "Loaded chat history for prompt building"
    );

    // Filter system messages if configured
    let filtered_messages: Vec<&ChatMessage> = if config.include_system_messages {
        messages.iter().collect()
    } else {
        messages.iter().filter(|m| m.role != "system").collect()
    };

    // Apply token budget truncation (drop oldest first)
    let (selected_messages, truncated) = apply_token_budget(
        &filtered_messages,
        new_user_message,
        config.max_history_tokens,
    );

    if truncated {
        debug!(
            session_id = %session_id,
            original_count = filtered_messages.len(),
            selected_count = selected_messages.len(),
            max_tokens = config.max_history_tokens,
            "Truncated chat history due to token budget"
        );
    }

    // Build the prompt text
    let prompt_text = format_prompt(&selected_messages, new_user_message);

    // Compute context hash from message IDs (for replay verification)
    let context_hash = compute_context_hash(&selected_messages);

    Ok(ChatPromptResult {
        prompt_text,
        message_count: selected_messages.len(),
        truncated,
        context_hash,
    })
}

/// Apply token budget truncation, keeping most recent messages.
///
/// Returns the selected messages and whether truncation occurred.
fn apply_token_budget<'a>(
    messages: &[&'a ChatMessage],
    new_message: &str,
    max_tokens: usize,
) -> (Vec<&'a ChatMessage>, bool) {
    // Reserve tokens for the new message
    let new_message_tokens = estimate_tokens(new_message) + estimate_tokens("[user]: ");
    let available_tokens = max_tokens.saturating_sub(new_message_tokens);

    let mut selected: Vec<&ChatMessage> = Vec::new();
    let mut total_tokens = 0;

    // Iterate from newest to oldest (reverse), then reverse result
    for msg in messages.iter().rev() {
        let msg_tokens = estimate_message_tokens(msg);

        if total_tokens + msg_tokens > available_tokens {
            // Would exceed budget, stop here
            break;
        }

        selected.push(msg);
        total_tokens += msg_tokens;
    }

    // Reverse to restore chronological order
    selected.reverse();

    let truncated = selected.len() < messages.len();

    (selected, truncated)
}

/// Estimate tokens for a message (role prefix + content)
fn estimate_message_tokens(msg: &ChatMessage) -> usize {
    // Format: "[role]: content\n"
    let prefix_len = format!("[{}]: ", msg.role).len();
    let content_len = msg.content.len();
    let newline_len = 1;

    (prefix_len + content_len + newline_len).div_ceil(CHARS_PER_TOKEN)
}

/// Estimate tokens for a string
fn estimate_tokens(s: &str) -> usize {
    s.len().div_ceil(CHARS_PER_TOKEN)
}

/// Format messages into a prompt with role markers.
///
/// Format:
/// ```text
/// [system]: System message
/// [user]: User message
/// [assistant]: Assistant response
/// [user]: New message
/// ```
fn format_prompt(history: &[&ChatMessage], new_message: &str) -> String {
    let mut prompt = String::new();

    // Add history messages
    for msg in history {
        prompt.push_str(&format!("[{}]: {}\n", msg.role, msg.content));
    }

    // Add the new user message
    prompt.push_str(&format!("[user]: {}", new_message));

    prompt
}

/// Compute BLAKE3 hash of message IDs for replay verification.
///
/// The hash is computed from sorted message IDs to ensure determinism.
fn compute_context_hash(messages: &[&ChatMessage]) -> String {
    let mut hasher = Hasher::new();

    // Hash message IDs in order (already chronologically sorted)
    for msg in messages {
        hasher.update(msg.id.as_bytes());
        hasher.update(b"|"); // Separator
    }

    let hash = hasher.finalize();
    // Return first 16 hex chars (64 bits) for brevity
    hex::encode(&hash.as_bytes()[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_message(id: &str, role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            id: id.to_string(),
            session_id: "test-session".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Some("2024-01-01T00:00:00Z".to_string()),
            metadata_json: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            sequence: 0,
            tenant_id: "tenant-test".to_string(),
        }
    }

    #[test]
    fn test_format_prompt_empty_history() {
        let history: Vec<&ChatMessage> = vec![];
        let result = format_prompt(&history, "Hello");
        assert_eq!(result, "[user]: Hello");
    }

    #[test]
    fn test_format_prompt_with_history() {
        let msg1 = make_message("1", "user", "First question");
        let msg2 = make_message("2", "assistant", "First answer");
        let history: Vec<&ChatMessage> = vec![&msg1, &msg2];

        let result = format_prompt(&history, "Second question");

        assert_eq!(
            result,
            "[user]: First question\n[assistant]: First answer\n[user]: Second question"
        );
    }

    #[test]
    fn test_format_prompt_with_system() {
        let msg1 = make_message("1", "system", "You are helpful");
        let msg2 = make_message("2", "user", "Hello");
        let msg3 = make_message("3", "assistant", "Hi there");
        let history: Vec<&ChatMessage> = vec![&msg1, &msg2, &msg3];

        let result = format_prompt(&history, "New message");

        assert_eq!(
            result,
            "[system]: You are helpful\n[user]: Hello\n[assistant]: Hi there\n[user]: New message"
        );
    }

    #[test]
    fn test_estimate_tokens() {
        // ~4 chars per token
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }

    #[test]
    fn test_token_budget_no_truncation() {
        let msg1 = make_message("1", "user", "Hi");
        let msg2 = make_message("2", "assistant", "Hello");
        let messages: Vec<&ChatMessage> = vec![&msg1, &msg2];

        // Large budget, no truncation
        let (selected, truncated) = apply_token_budget(&messages, "New", 1000);

        assert_eq!(selected.len(), 2);
        assert!(!truncated);
    }

    #[test]
    fn test_token_budget_with_truncation() {
        let msg1 = make_message("1", "user", "This is a longer message that takes up tokens");
        let msg2 = make_message("2", "assistant", "This is also a longer response");
        let msg3 = make_message("3", "user", "Another message");
        let messages: Vec<&ChatMessage> = vec![&msg1, &msg2, &msg3];

        // Small budget forces truncation
        let (selected, truncated) = apply_token_budget(&messages, "New message", 20);

        assert!(selected.len() < 3);
        assert!(truncated);
        // Most recent messages should be kept
        if !selected.is_empty() {
            assert_eq!(selected.last().unwrap().id, "3");
        }
    }

    #[test]
    fn test_context_hash_determinism() {
        let msg1 = make_message("msg-001", "user", "Hello");
        let msg2 = make_message("msg-002", "assistant", "Hi");
        let messages: Vec<&ChatMessage> = vec![&msg1, &msg2];

        let hash1 = compute_context_hash(&messages);
        let hash2 = compute_context_hash(&messages);

        assert_eq!(hash1, hash2, "Context hash should be deterministic");
    }

    #[test]
    fn test_context_hash_different_for_different_messages() {
        let msg1 = make_message("msg-001", "user", "Hello");
        let msg2 = make_message("msg-002", "user", "Hello");
        let messages1: Vec<&ChatMessage> = vec![&msg1];
        let messages2: Vec<&ChatMessage> = vec![&msg2];

        let hash1 = compute_context_hash(&messages1);
        let hash2 = compute_context_hash(&messages2);

        assert_ne!(
            hash1, hash2,
            "Different message IDs should produce different hashes"
        );
    }

    #[test]
    fn test_context_hash_order_matters() {
        let msg1 = make_message("msg-001", "user", "Hello");
        let msg2 = make_message("msg-002", "assistant", "Hi");
        let messages_forward: Vec<&ChatMessage> = vec![&msg1, &msg2];
        let messages_backward: Vec<&ChatMessage> = vec![&msg2, &msg1];

        let hash_forward = compute_context_hash(&messages_forward);
        let hash_backward = compute_context_hash(&messages_backward);

        assert_ne!(
            hash_forward, hash_backward,
            "Order of messages should affect hash"
        );
    }
}
