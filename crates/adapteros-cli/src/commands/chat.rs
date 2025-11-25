//! Interactive chat CLI with streaming support

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use tracing::{error, info};

/// Inference request for chat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub prompt: String,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub stream: bool,
    pub adapter_stack: Option<String>,
}

/// Inference response chunk (streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceChunk {
    pub id: String,
    pub object: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub delta: Delta,
    pub index: usize,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    pub content: Option<String>,
    pub role: Option<String>,
}

/// Chat session response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub session_id: String,
    pub stack_id: Option<String>,
    pub created_at: String,
    pub message_count: usize,
}

#[derive(Debug, Subcommand, Clone)]
pub enum ChatCommand {
    /// Start interactive chat (REPL mode)
    #[command(
        after_help = "Examples:\n  aosctl chat interactive --stack my-stack\n  aosctl chat interactive\n\n  Type 'exit' or press Ctrl+D to quit\n  Type '/clear' to clear screen\n  Type '/stack <id>' to switch stacks"
    )]
    Interactive {
        /// Stack ID to use
        #[arg(long)]
        stack: Option<String>,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,

        /// Show verbose output (activated adapters, etc.)
        #[arg(long)]
        verbose: bool,
    },

    /// Single prompt mode (non-interactive)
    #[command(
        after_help = "Examples:\n  aosctl chat prompt --text \"Explain async in Rust\" --stack my-stack\n  aosctl chat prompt --text \"Write hello world\" --max-tokens 100"
    )]
    Prompt {
        /// Prompt text
        #[arg(long)]
        text: String,

        /// Stack ID to use
        #[arg(long)]
        stack: Option<String>,

        /// Maximum tokens to generate
        #[arg(long, default_value = "200")]
        max_tokens: usize,

        /// Temperature (0.0-1.0)
        #[arg(long, default_value = "0.7")]
        temperature: f32,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// List saved chat sessions
    #[command(after_help = "Examples:\n  aosctl chat list\n  aosctl chat list --json")]
    List {
        /// Output format
        #[arg(long)]
        json: bool,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// View chat session history
    #[command(
        after_help = "Examples:\n  aosctl chat history <session-id>\n  aosctl chat history <session-id> --json"
    )]
    History {
        /// Session ID
        session_id: String,

        /// Output format
        #[arg(long)]
        json: bool,

        /// Base URL for API
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },
}

/// Handle chat commands
pub async fn handle_chat_command(cmd: ChatCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_chat_command_name(&cmd);

    info!(command = ?cmd, "Handling chat command");

    // Emit telemetry
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await;

    match cmd {
        ChatCommand::Interactive {
            stack,
            base_url,
            verbose,
        } => run_interactive_chat(stack, &base_url, verbose, output).await,
        ChatCommand::Prompt {
            text,
            stack,
            max_tokens,
            temperature,
            base_url,
        } => run_single_prompt(&text, stack, max_tokens, temperature, &base_url, output).await,
        ChatCommand::List { json, base_url } => list_chat_sessions(json, &base_url, output).await,
        ChatCommand::History {
            session_id,
            json,
            base_url,
        } => show_chat_history(&session_id, json, &base_url, output).await,
    }
}

/// Get chat command name for telemetry
fn get_chat_command_name(cmd: &ChatCommand) -> String {
    match cmd {
        ChatCommand::Interactive { .. } => "chat_interactive",
        ChatCommand::Prompt { .. } => "chat_prompt",
        ChatCommand::List { .. } => "chat_list",
        ChatCommand::History { .. } => "chat_history",
    }
    .to_string()
}

/// Run interactive chat REPL
async fn run_interactive_chat(
    stack: Option<String>,
    base_url: &str,
    verbose: bool,
    output: &OutputWriter,
) -> Result<()> {
    info!(stack = ?stack, "Starting interactive chat");

    output.info("Starting interactive chat mode");
    if let Some(ref stack_id) = stack {
        output.kv("Using stack", stack_id);
    } else {
        output.result("Using base model (no stack)");
    }
    output.blank();
    output.result("Commands:");
    output.result("  exit, quit, Ctrl+D - Exit chat");
    output.result("  /clear - Clear screen");
    output.result("  /stack <id> - Switch adapter stack");
    output.blank();

    let mut current_stack = stack;

    // Use simple stdin reading (rustyline not available in dependencies)
    loop {
        // Print prompt
        print!("You> ");
        io::stdout().flush().unwrap();

        // Read input
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                // EOF (Ctrl+D)
                output.blank();
                output.info("Goodbye!");
                break;
            }
            Ok(_) => {
                let input = input.trim();

                // Handle empty input
                if input.is_empty() {
                    continue;
                }

                // Handle commands
                if input == "exit" || input == "quit" {
                    output.blank();
                    output.info("Goodbye!");
                    break;
                } else if input == "/clear" {
                    // Clear screen (ANSI escape sequence)
                    print!("\x1B[2J\x1B[H");
                    continue;
                } else if input.starts_with("/stack ") {
                    let stack_id = input.strip_prefix("/stack ").unwrap().trim();
                    if stack_id.is_empty() {
                        current_stack = None;
                        output.info("Switched to base model (no stack)");
                    } else {
                        current_stack = Some(stack_id.to_string());
                        output.info(&format!("Switched to stack: {}", stack_id));
                    }
                    continue;
                }

                // Send inference request
                match send_streaming_inference(input, current_stack.clone(), base_url, verbose)
                    .await
                {
                    Ok(_) => {
                        output.blank();
                    }
                    Err(e) => {
                        error!(error = %e, "Inference failed");
                        output.error(&format!("Error: {}", e));
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to read input");
                return Err(AosError::Io(format!("Failed to read input: {}", e)));
            }
        }
    }

    Ok(())
}

/// Send streaming inference request
async fn send_streaming_inference(
    prompt: &str,
    stack: Option<String>,
    base_url: &str,
    verbose: bool,
) -> Result<()> {
    let request = InferenceRequest {
        prompt: prompt.to_string(),
        max_tokens: Some(500),
        temperature: Some(0.7),
        stream: true,
        adapter_stack: stack,
    };

    let client = reqwest::Client::new();
    let url = format!("{}/v1/infer", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Inference failed: {} {}",
            status, text
        )));
    }

    // Print streaming response
    print!("Assistant> ");
    io::stdout().flush().unwrap();

    let body = resp
        .text()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    // Parse SSE format (data: {...}\n\n)
    for line in body.lines() {
        if line.starts_with("data: ") {
            let json_str = line.strip_prefix("data: ").unwrap();

            if json_str == "[DONE]" {
                println!(); // Newline after streaming
                break;
            }

            // Parse JSON chunk
            match serde_json::from_str::<InferenceChunk>(json_str) {
                Ok(chunk) => {
                    for choice in &chunk.choices {
                        if let Some(ref content) = choice.delta.content {
                            print!("{}", content);
                            io::stdout().flush().unwrap();
                        }
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("\nWarning: Failed to parse chunk: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Run single prompt (non-interactive)
async fn run_single_prompt(
    text: &str,
    stack: Option<String>,
    max_tokens: usize,
    temperature: f32,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(prompt = %text, stack = ?stack, "Running single prompt");

    let request = InferenceRequest {
        prompt: text.to_string(),
        max_tokens: Some(max_tokens),
        temperature: Some(temperature),
        stream: true,
        adapter_stack: stack,
    };

    let client = reqwest::Client::new();
    let url = format!("{}/v1/infer", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Inference failed: {} {}",
            status, text
        )));
    }

    // Print streaming response
    let body = resp
        .text()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    // Parse SSE format
    for line in body.lines() {
        if line.starts_with("data: ") {
            let json_str = line.strip_prefix("data: ").unwrap();

            if json_str == "[DONE]" {
                println!();
                break;
            }

            // Parse JSON chunk
            match serde_json::from_str::<InferenceChunk>(json_str) {
                Ok(chunk) => {
                    for choice in &chunk.choices {
                        if let Some(ref content) = choice.delta.content {
                            print!("{}", content);
                            io::stdout().flush().unwrap();
                        }
                    }
                }
                Err(_) => {
                    // Ignore parse errors for single prompt mode
                }
            }
        }
    }

    Ok(())
}

/// List saved chat sessions
async fn list_chat_sessions(json: bool, base_url: &str, output: &OutputWriter) -> Result<()> {
    info!("Listing chat sessions");

    // Note: This endpoint may not exist yet in the API
    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/sessions", base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to list sessions: {} {}",
            status, text
        )));
    }

    let sessions: Vec<ChatSession> = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    if json {
        output.result(&serde_json::to_string_pretty(&sessions)?);
    } else {
        if sessions.is_empty() {
            output.info("No chat sessions found");
            return Ok(());
        }

        output.info(&format!("Found {} chat sessions:", sessions.len()));
        output.blank();

        for session in &sessions {
            output.kv("Session ID", &session.session_id);
            output.kv("Stack", session.stack_id.as_deref().unwrap_or("base model"));
            output.kv("Created", &session.created_at);
            output.kv("Messages", &session.message_count.to_string());
            output.blank();
        }
    }

    Ok(())
}

/// Show chat session history
async fn show_chat_history(
    session_id: &str,
    json: bool,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(session_id = %session_id, "Showing chat history");

    // Note: This endpoint may not exist yet in the API
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/chat/sessions/{}/history",
        base_url.trim_end_matches('/'),
        session_id
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Failed to get history: {} {}",
            status, text
        )));
    }

    let history: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    if json {
        output.result(&serde_json::to_string_pretty(&history)?);
    } else {
        output.info(&format!("Chat history for session: {}", session_id));
        output.blank();

        if let Some(messages) = history.get("messages").and_then(|m| m.as_array()) {
            for (i, message) in messages.iter().enumerate() {
                let role = message
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown");
                let content = message
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");

                output.result(&format!("[{}] {}:", i + 1, role));
                output.result(content);
                output.blank();
            }
        } else {
            output.warning("No messages found in session");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_chat_command_name() {
        assert_eq!(
            get_chat_command_name(&ChatCommand::Interactive {
                stack: None,
                base_url: "http://localhost:8080".to_string(),
                verbose: false,
            }),
            "chat_interactive"
        );
        assert_eq!(
            get_chat_command_name(&ChatCommand::Prompt {
                text: "test".to_string(),
                stack: None,
                max_tokens: 100,
                temperature: 0.7,
                base_url: "http://localhost:8080".to_string(),
            }),
            "chat_prompt"
        );
    }

    #[test]
    fn test_inference_request_serialization() {
        let req = InferenceRequest {
            prompt: "test".to_string(),
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: true,
            adapter_stack: Some("stack-1".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: InferenceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.prompt, deserialized.prompt);
        assert_eq!(req.max_tokens, deserialized.max_tokens);
        assert_eq!(req.stream, deserialized.stream);
    }
}
