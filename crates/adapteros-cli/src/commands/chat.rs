//! Interactive chat CLI with streaming support
//!
//! This module provides a robust CLI interface for chatting with the AdapterOS
//! inference runtime. It supports both interactive REPL and one-shot modes,
//! with configurable endpoints, model/stack selection, and graceful error handling.

use crate::error_codes::{get, ECode};
use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use chrono::{SecondsFormat, Utc};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Default timeout for HTTP requests (in seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum number of connection retries
const MAX_RETRIES: u32 = 3;

/// Delay between retries (in milliseconds)
const RETRY_DELAY_MS: u64 = 1000;

/// Inference request for chat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub prompt: String,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_mode: Option<bool>,
}

/// Configuration for chat client
#[derive(Debug, Clone)]
pub struct ChatConfig {
    pub base_url: String,
    pub timeout: Duration,
    pub retries: u32,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080/api".to_string(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retries: MAX_RETRIES,
        }
    }
}

/// Create an HTTP client with appropriate timeout configuration
fn create_http_client(timeout: Duration) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .build()
}

/// Parameters for inference requests
#[derive(Debug, Clone)]
struct InferenceParams {
    prompt: String,
    stack: Option<String>,
    model: Option<String>,
    max_tokens: Option<usize>,
    temperature: Option<f32>,
}

/// User-friendly error messages for common failure scenarios
fn format_user_error(err: &reqwest::Error, base_url: &str) -> String {
    if err.is_connect() {
        let error_code = get(ECode::E7004);
        format!(
            "\x1b[1;31mError {}: {}\x1b[0m\n\n\
             Could not connect to the inference server at {}.\n\n\
             \x1b[1mCause:\x1b[0m {}\n\n\
             \x1b[1mFix:\x1b[0m\n{}",
            error_code.code, error_code.title, base_url, error_code.cause, error_code.fix
        )
    } else if err.is_timeout() {
        format!(
            "\x1b[1;31mError: Request Timeout\x1b[0m\n\n\
             Request to {} timed out.\n\n\
             \x1b[1mFix:\x1b[0m\n\
             1. The server may be overloaded. Retry the request.\n\
             2. Increase timeout: \x1b[1m--timeout 60\x1b[0m\n\
             3. Check server status: \x1b[1m./aosctl status\x1b[0m",
            base_url
        )
    } else if err.is_request() {
        format!(
            "\x1b[1;31mError: Request Failed\x1b[0m\n\n\
             Failed to send request to {}.\n\n\
             \x1b[1mFix:\x1b[0m\n\
             1. Check your network connection\n\
             2. Verify server status: \x1b[1m./aosctl status\x1b[0m\n\
             3. Check server logs: \x1b[1mtail -f var/log/server.log\x1b[0m",
            base_url
        )
    } else {
        format!("Request failed: {}", err)
    }
}

/// Format error for model-related issues with actionable hints
fn format_model_error(_status: reqwest::StatusCode, body: &str) -> Option<String> {
    // Check for common model-related errors
    if body.contains("no models") || body.contains("model not found") || body.contains("NoModels") {
        let error_code = get(ECode::E6010);
        return Some(format!(
            "\x1b[1;31mError {}: {}\x1b[0m\n\n\
             \x1b[1mCause:\x1b[0m {}\n\n\
             \x1b[1mFix:\x1b[0m\n{}",
            error_code.code, error_code.title, error_code.cause, error_code.fix
        ));
    }

    if body.contains("no workers") || body.contains("worker unavailable") {
        let error_code = get(ECode::E7003);
        return Some(format!(
            "\x1b[1;31mError {}: {}\x1b[0m\n\n\
             \x1b[1mCause:\x1b[0m {}\n\n\
             \x1b[1mFix:\x1b[0m\n{}",
            error_code.code, error_code.title, error_code.cause, error_code.fix
        ));
    }

    if body.contains("adapter not loaded") || body.contains("AdapterNotLoaded") {
        let error_code = get(ECode::E6011);
        return Some(format!(
            "\x1b[1;31mError {}: {}\x1b[0m\n\n\
             \x1b[1mCause:\x1b[0m {}\n\n\
             \x1b[1mFix:\x1b[0m\n{}",
            error_code.code, error_code.title, error_code.cause, error_code.fix
        ));
    }

    // Return None if no specific error pattern matched
    None
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
    pub id: String,
    pub name: String,
    pub stack_id: Option<String>,
    pub source_type: Option<String>,
    pub created_at: String,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Subcommand, Clone)]
pub enum ChatCommand {
    /// Start interactive chat (REPL mode)
    #[command(
        after_help = "Examples:\n  aosctl chat interactive --stack my-stack\n  aosctl chat interactive --model Qwen2.5-7B-Instruct\n  aosctl chat interactive\n\n  REPL Commands:\n    exit, quit, Ctrl+D - Exit chat\n    /help              - Show available commands\n    /clear             - Clear screen\n    /stack <id>        - Switch adapter stack\n    /model <id>        - Switch model\n    /status            - Show current configuration"
    )]
    Interactive {
        /// Stack ID to use (adapter stack for specialized behavior)
        #[arg(long)]
        stack: Option<String>,

        /// Model ID to use (e.g., Qwen2.5-7B-Instruct)
        #[arg(long, short = 'm')]
        model: Option<String>,

        /// Base URL for API
        #[arg(
            long,
            env = "AOS_SERVER_URL",
            default_value = "http://127.0.0.1:8080/api"
        )]
        base_url: String,

        /// Request timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,

        /// Show verbose output (activated adapters, etc.)
        #[arg(long, short = 'v')]
        verbose: bool,

        /// Mark session as owned by system assistant (source_type=owner_system)
        #[arg(long)]
        owner_system: bool,
    },

    /// Single prompt mode (non-interactive)
    #[command(
        after_help = "Examples:\n  aosctl chat prompt --text \"Explain async in Rust\" --stack my-stack\n  aosctl chat prompt --text \"Write hello world\" --max-tokens 100\n  aosctl chat prompt --text \"Summarize this\" --model Qwen2.5-7B-Instruct"
    )]
    Prompt {
        /// Prompt text
        #[arg(long, short = 't')]
        text: String,

        /// Stack ID to use (adapter stack for specialized behavior)
        #[arg(long)]
        stack: Option<String>,

        /// Model ID to use (e.g., Qwen2.5-7B-Instruct)
        #[arg(long, short = 'm')]
        model: Option<String>,

        /// Maximum tokens to generate
        #[arg(long, default_value = "200")]
        max_tokens: usize,

        /// Temperature (0.0-1.0)
        #[arg(long, default_value = "0.7")]
        temperature: f32,

        /// Base URL for API
        #[arg(
            long,
            env = "AOS_SERVER_URL",
            default_value = "http://127.0.0.1:8080/api"
        )]
        base_url: String,

        /// Request timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,

        /// Mark session as owned by system assistant (source_type=owner_system)
        #[arg(long)]
        owner_system: bool,
    },

    /// List saved chat sessions
    #[command(after_help = "Examples:\n  aosctl chat list\n  aosctl chat list --json")]
    List {
        /// Output format
        #[arg(long)]
        json: bool,

        /// Base URL for API
        #[arg(
            long,
            env = "AOS_SERVER_URL",
            default_value = "http://127.0.0.1:8080/api"
        )]
        base_url: String,

        /// Request timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,
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
        #[arg(
            long,
            env = "AOS_SERVER_URL",
            default_value = "http://127.0.0.1:8080/api"
        )]
        base_url: String,

        /// Request timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,
    },
}

/// Handle chat commands
pub async fn handle_chat_command(cmd: ChatCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_chat_command_name(&cmd);

    info!(command = ?cmd, "Handling chat command");

    // Emit telemetry
    if let Err(e) = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await {
        debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        ChatCommand::Interactive {
            stack,
            model,
            base_url,
            timeout,
            verbose,
            owner_system,
        } => {
            let config = ChatConfig {
                base_url: base_url.clone(),
                timeout: Duration::from_secs(timeout),
                retries: MAX_RETRIES,
            };
            run_interactive_chat(stack, model, config, verbose, owner_system, output).await
        }
        ChatCommand::Prompt {
            text,
            stack,
            model,
            max_tokens,
            temperature,
            base_url,
            timeout,
            owner_system,
        } => {
            let config = ChatConfig {
                base_url: base_url.clone(),
                timeout: Duration::from_secs(timeout),
                retries: MAX_RETRIES,
            };
            run_single_prompt(
                &text,
                stack,
                model,
                max_tokens,
                temperature,
                config,
                owner_system,
                output,
            )
            .await
        }
        ChatCommand::List {
            json,
            base_url,
            timeout,
        } => {
            let config = ChatConfig {
                base_url: base_url.clone(),
                timeout: Duration::from_secs(timeout),
                retries: MAX_RETRIES,
            };
            list_chat_sessions(json, &config, output).await
        }
        ChatCommand::History {
            session_id,
            json,
            base_url,
            timeout,
        } => {
            let config = ChatConfig {
                base_url: base_url.clone(),
                timeout: Duration::from_secs(timeout),
                retries: MAX_RETRIES,
            };
            show_chat_history(&session_id, json, &config, output).await
        }
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

/// Print the REPL help message
fn print_repl_help(output: &OutputWriter) {
    output.info("Available commands:");
    output.result("  exit, quit, Ctrl+D - Exit chat");
    output.result("  /help              - Show this help message");
    output.result("  /clear             - Clear screen");
    output.result("  /stack <id>        - Switch adapter stack (empty to use base model)");
    output.result("  /model <id>        - Switch model");
    output.result("  /status            - Show current configuration");
    output.blank();
}

/// Print the current configuration status
fn print_status(
    output: &OutputWriter,
    stack: &Option<String>,
    model: &Option<String>,
    config: &ChatConfig,
) {
    output.info("Current configuration:");
    output.kv("Server", &config.base_url);
    output.kv("Stack", stack.as_deref().unwrap_or("(base model)"));
    output.kv("Model", model.as_deref().unwrap_or("(default)"));
    output.kv("Timeout", &format!("{}s", config.timeout.as_secs()));
    output.blank();
}

/// Run interactive chat REPL
async fn run_interactive_chat(
    stack: Option<String>,
    model: Option<String>,
    config: ChatConfig,
    verbose: bool,
    owner_system: bool,
    output: &OutputWriter,
) -> Result<()> {
    info!(stack = ?stack, model = ?model, "Starting interactive chat");

    output.info("Starting interactive chat mode");
    if let Some(ref stack_id) = stack {
        output.kv("Stack", stack_id);
    }
    if let Some(ref model_id) = model {
        output.kv("Model", model_id);
    }
    if stack.is_none() && model.is_none() {
        output.result("Using base model (no stack/model specified)");
    }
    output.blank();
    output.result("Type /help for available commands");
    output.blank();

    let mut current_stack = stack;
    let mut current_model = model;
    let session_source = if owner_system { "owner_system" } else { "cli" };
    let session_id = create_cli_session(session_source, current_stack.clone(), &config).await?;

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
                } else if input == "/help" {
                    print_repl_help(output);
                    continue;
                } else if input == "/clear" {
                    // Clear screen (ANSI escape sequence)
                    print!("\x1B[2J\x1B[H");
                    continue;
                } else if input == "/status" {
                    print_status(output, &current_stack, &current_model, &config);
                    continue;
                } else if input.starts_with("/stack") {
                    let stack_id = input.strip_prefix("/stack").unwrap().trim();
                    if stack_id.is_empty() {
                        current_stack = None;
                        output.info("Switched to base model (no stack)");
                    } else {
                        current_stack = Some(stack_id.to_string());
                        output.info(format!("Switched to stack: {}", stack_id));
                    }
                    continue;
                } else if input.starts_with("/model") {
                    let model_id = input.strip_prefix("/model").unwrap().trim();
                    if model_id.is_empty() {
                        current_model = None;
                        output.info("Switched to default model");
                    } else {
                        current_model = Some(model_id.to_string());
                        output.info(format!("Switched to model: {}", model_id));
                    }
                    continue;
                } else if input.starts_with('/') {
                    output.warning(format!("Unknown command: {}. Type /help for help.", input));
                    continue;
                }

                // Send inference request
                match run_cli_turn(
                    &session_id,
                    input,
                    current_stack.clone(),
                    current_model.clone(),
                    &config,
                    verbose,
                )
                .await
                {
                    Ok(_) => output.blank(),
                    Err(e) => {
                        error!(error = %e, "Inference failed");
                        // Provide user-friendly error message
                        let error_msg = format!("{}", e);
                        if error_msg.contains("connection") || error_msg.contains("HTTP") {
                            output.error("Failed to communicate with the server.");
                            output.error("Check if the server is running and try again.");
                        } else {
                            output.error(format!("Error: {}", e));
                        }
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

/// Send streaming inference request with retry logic
#[allow(clippy::too_many_arguments)]
async fn send_streaming_inference(
    params: InferenceParams,
    config: &ChatConfig,
    verbose: bool,
    output: Option<&OutputWriter>,
) -> Result<String> {
    let request = InferenceRequest {
        prompt: params.prompt.clone(),
        max_tokens: params.max_tokens,
        temperature: params.temperature,
        stream: true,
        adapter_stack: params.stack,
        model: params.model,
        reasoning_mode: None,
    };

    let client = create_http_client(config.timeout)
        .map_err(|e| AosError::Io(format!("Failed to create HTTP client: {}", e)))?;
    let url = format!("{}/v1/infer", config.base_url.trim_end_matches('/'));

    // Retry logic for transient failures
    let mut last_error = None;
    for attempt in 0..config.retries {
        if attempt > 0 {
            debug!(attempt = attempt, "Retrying inference request");
            tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
        }

        match client.post(&url).json(&request).send().await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    // Don't retry client errors (4xx)
                    if status.is_client_error() {
                        return Err(AosError::Http(format!(
                            "Inference failed ({}): {}",
                            status, text
                        )));
                    }
                    // Retry server errors (5xx)
                    last_error = Some(AosError::Http(format!(
                        "Server error ({}): {}",
                        status, text
                    )));
                    continue;
                }

                let should_print_stream = output
                    .map(|o| !o.is_quiet() && !o.is_json())
                    .unwrap_or(true);

                // Print streaming response
                if should_print_stream {
                    print!("Assistant> ");
                    io::stdout().flush().unwrap();
                }

                let body = resp
                    .text()
                    .await
                    .map_err(|e| AosError::Http(e.to_string()))?;

                let mut full_text = String::new();

                // Parse SSE format (data: {...}\n\n)
                for line in body.lines() {
                    if line.starts_with("data: ") {
                        let json_str = line.strip_prefix("data: ").unwrap();

                        if json_str == "[DONE]" {
                            if should_print_stream {
                                println!(); // Newline after streaming
                            }
                            break;
                        }

                        // Parse JSON chunk
                        match serde_json::from_str::<InferenceChunk>(json_str) {
                            Ok(chunk) => {
                                for choice in &chunk.choices {
                                    if let Some(ref content) = choice.delta.content {
                                        if should_print_stream {
                                            print!("{}", content);
                                            io::stdout().flush().unwrap();
                                        }
                                        full_text.push_str(content);
                                    }
                                }
                            }
                            Err(e) => {
                                if verbose {
                                    warn!(error = %e, "Failed to parse chunk");
                                }
                            }
                        }
                    }
                }

                if let Some(out) = output {
                    out.result(&full_text);
                }

                return Ok(full_text);
            }
            Err(e) => {
                // Connection errors are retryable
                if e.is_connect() || e.is_timeout() {
                    warn!(attempt = attempt, error = %e, "Request failed, will retry");
                    last_error = Some(AosError::Io(format_user_error(&e, &config.base_url)));
                    continue;
                }
                // Other errors are not retryable
                return Err(AosError::Io(format_user_error(&e, &config.base_url)));
            }
        }
    }

    // All retries exhausted
    Err(last_error.unwrap_or_else(|| AosError::Io("Request failed after retries".to_string())))
}

/// Run single prompt (non-interactive)
#[allow(clippy::too_many_arguments)]
async fn run_single_prompt(
    text: &str,
    stack: Option<String>,
    model: Option<String>,
    max_tokens: usize,
    temperature: f32,
    config: ChatConfig,
    owner_system: bool,
    output: &OutputWriter,
) -> Result<()> {
    info!(prompt = %text, stack = ?stack, model = ?model, "Running single prompt");

    let source_type = if owner_system {
        "owner_system"
    } else {
        "cli_prompt"
    };
    let session_id = create_cli_session(source_type, stack.clone(), &config).await?;
    add_cli_message(&session_id, "user", text, &config).await?;
    let params = InferenceParams {
        prompt: text.to_string(),
        stack: stack.clone(),
        model,
        max_tokens: Some(max_tokens),
        temperature: Some(temperature),
    };
    let response_text = send_streaming_inference(params, &config, false, Some(output)).await?;
    add_cli_message(&session_id, "assistant", &response_text, &config).await?;
    output.blank();

    Ok(())
}

/// List saved chat sessions
async fn list_chat_sessions(json: bool, config: &ChatConfig, output: &OutputWriter) -> Result<()> {
    info!("Listing chat sessions");

    let client = create_http_client(config.timeout)
        .map_err(|e| AosError::Io(format!("Failed to create HTTP client: {}", e)))?;
    let url = format!("{}/v1/chat/sessions", config.base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format_user_error(&e, &config.base_url)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "Failed to list sessions ({}): {}",
            status, text
        )));
    }

    let sessions: Vec<ChatSession> = resp
        .json()
        .await
        .map_err(|e| AosError::Http(format!("Invalid response format: {}", e)))?;

    if json {
        output.result(&serde_json::to_string_pretty(&sessions)?);
    } else {
        if sessions.is_empty() {
            output.info("No chat sessions found");
            return Ok(());
        }

        output.info(format!("Found {} chat sessions:", sessions.len()));
        output.blank();

        for session in &sessions {
            output.kv("Session ID", &session.id);
            output.kv("Name", &session.name);
            output.kv(
                "Source",
                session.source_type.as_deref().unwrap_or("general"),
            );
            output.kv("Stack", session.stack_id.as_deref().unwrap_or("base model"));
            output.kv("Created", &session.created_at);
            output.kv("Last activity", &session.last_activity_at);
            output.blank();
        }
    }

    Ok(())
}

/// Show chat session history
async fn show_chat_history(
    session_id: &str,
    json: bool,
    config: &ChatConfig,
    output: &OutputWriter,
) -> Result<()> {
    info!(session_id = %session_id, "Showing chat history");

    let client = create_http_client(config.timeout)
        .map_err(|e| AosError::Io(format!("Failed to create HTTP client: {}", e)))?;
    let url = format!(
        "{}/v1/chat/sessions/{}/messages",
        config.base_url.trim_end_matches('/'),
        session_id
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format_user_error(&e, &config.base_url)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        if status.as_u16() == 404 {
            return Err(AosError::Http(format!(
                "Session '{}' not found. Use 'aosctl chat list' to see available sessions.",
                session_id
            )));
        }
        return Err(AosError::Http(format!(
            "Failed to get history ({}): {}",
            status, text
        )));
    }

    let history: Vec<ChatMessageResponse> = resp
        .json()
        .await
        .map_err(|e| AosError::Http(format!("Invalid response format: {}", e)))?;

    if json {
        output.result(&serde_json::to_string_pretty(&history)?);
    } else {
        output.info(format!("Chat history for session: {}", session_id));
        output.blank();

        if history.is_empty() {
            output.warning("No messages found in session");
            return Ok(());
        }
        for (i, message) in history.iter().enumerate() {
            output.result(format!(
                "[{}] {} ({}):",
                i + 1,
                message.role,
                message.created_at
            ));
            output.result(&message.content);
            output.blank();
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessageResponse {
    id: String,
    session_id: String,
    tenant_id: String,
    role: String,
    content: String,
    timestamp: String,
    created_at: String,
    sequence: i64,
    #[serde(default)]
    metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateSessionRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateSessionResponse {
    session_id: String,
}

async fn create_cli_session(
    source_type: &str,
    stack: Option<String>,
    config: &ChatConfig,
) -> Result<String> {
    let client = create_http_client(config.timeout)
        .map_err(|e| AosError::Io(format!("Failed to create HTTP client: {}", e)))?;
    let url = format!("{}/v1/chat/sessions", config.base_url.trim_end_matches('/'));
    let name = format!(
        "[{}] {}",
        source_type,
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
    );
    let body = CreateSessionRequest {
        name,
        stack_id: stack.clone(),
        collection_id: None,
        source_type: Some(source_type.to_string()),
    };

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AosError::Io(format_user_error(&e, &config.base_url)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "Failed to create chat session ({}): {}",
            status, text
        )));
    }

    let created: CreateSessionResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(format!("Invalid response format: {}", e)))?;
    Ok(created.session_id)
}

async fn add_cli_message(
    session_id: &str,
    role: &str,
    content: &str,
    config: &ChatConfig,
) -> Result<()> {
    let client = create_http_client(config.timeout)
        .map_err(|e| AosError::Io(format!("Failed to create HTTP client: {}", e)))?;
    let url = format!(
        "{}/v1/chat/sessions/{}/messages",
        config.base_url.trim_end_matches('/'),
        session_id
    );
    let body = serde_json::json!({
        "role": role,
        "content": content,
    });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AosError::Io(format_user_error(&e, &config.base_url)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "Failed to add message ({}): {}",
            status, text
        )));
    }

    Ok(())
}

async fn run_cli_turn(
    session_id: &str,
    user_input: &str,
    stack: Option<String>,
    model: Option<String>,
    config: &ChatConfig,
    verbose: bool,
) -> Result<()> {
    add_cli_message(session_id, "user", user_input, config).await?;
    let params = InferenceParams {
        prompt: user_input.to_string(),
        stack: stack.clone(),
        model,
        max_tokens: Some(500),
        temperature: Some(0.7),
    };
    let assistant_text = send_streaming_inference(params, config, verbose, None).await?;
    add_cli_message(session_id, "assistant", &assistant_text, config).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_store::{save_auth, AuthStore};
    use crate::output::{OutputMode, OutputWriter};
    use adapteros_platform::common::PlatformUtils;
    use axum::{
        extract::Path,
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::Value;
    use serial_test::serial;
    use std::{env, sync::Arc, sync::Mutex as StdMutex};
    use tempfile::NamedTempFile;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    /// Helper to create a test ChatConfig
    fn test_config(base_url: &str) -> ChatConfig {
        ChatConfig {
            base_url: base_url.to_string(),
            timeout: Duration::from_secs(5),
            retries: 1, // Single retry for faster tests
        }
    }

    #[test]
    fn test_get_chat_command_name() {
        assert_eq!(
            get_chat_command_name(&ChatCommand::Interactive {
                stack: None,
                model: None,
                base_url: "http://localhost:8080".to_string(),
                timeout: 30,
                verbose: false,
                owner_system: false,
            }),
            "chat_interactive"
        );
        assert_eq!(
            get_chat_command_name(&ChatCommand::Prompt {
                text: "test".to_string(),
                stack: None,
                model: None,
                max_tokens: 100,
                temperature: 0.7,
                base_url: "http://localhost:8080".to_string(),
                timeout: 30,
                owner_system: false,
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
            model: None,
            reasoning_mode: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: InferenceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.prompt, deserialized.prompt);
        assert_eq!(req.max_tokens, deserialized.max_tokens);
        assert_eq!(req.stream, deserialized.stream);
    }

    #[test]
    fn test_inference_request_with_model() {
        let req = InferenceRequest {
            prompt: "test".to_string(),
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: true,
            adapter_stack: None,
            model: Some("Qwen2.5-7B-Instruct".to_string()),
            reasoning_mode: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Qwen2.5-7B-Instruct"));

        let deserialized: InferenceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.model, deserialized.model);
    }

    #[test]
    fn test_chat_config_default() {
        let config = ChatConfig::default();
        assert_eq!(config.base_url, "http://127.0.0.1:8080/api");
        assert_eq!(config.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));
        assert_eq!(config.retries, MAX_RETRIES);
    }

    #[test]
    fn test_format_user_error_messages() {
        // Test that error formatting produces helpful messages
        // Note: We can't easily test reqwest errors directly, but we can verify
        // the function signature and basic behavior
        let _base_url = "http://localhost:8080";

        // The function should exist and be callable
        // This is a compile-time check
        fn _check_format_user_error(_err: &reqwest::Error, _base_url: &str) -> String {
            format_user_error(_err, _base_url)
        }
    }

    #[tokio::test]
    #[serial]
    async fn prompt_flow_posts_cli_prompt_and_messages() {
        let state = Arc::new(MockServerState::default());
        let app = mock_chat_router(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().unwrap();
        let make_svc = app.into_make_service();
        let server = tokio::spawn(async move {
            axum::serve(listener, make_svc)
                .await
                .expect("mock server run")
        });
        let base_url = format!("http://{}", addr);
        let config = test_config(&base_url);

        let sink = Arc::new(StdMutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Json, false, sink.clone());

        run_single_prompt("hello cli", None, None, 64, 0.5, config, false, &output)
            .await
            .expect("prompt flow");

        // Session creation recorded with cli_prompt
        let sessions = state.sessions.lock().await;
        let session_body = sessions.last().expect("session request");
        assert_eq!(
            session_body["source_type"],
            Value::String("cli_prompt".to_string())
        );

        // Messages recorded for user + assistant
        let messages = state.messages.lock().await;
        let roles: Vec<String> = messages
            .iter()
            .map(|m| m["role"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(roles, vec!["user".to_string(), "assistant".to_string()]);

        let captured = sink.lock().unwrap().join("\n");
        assert!(
            captured.contains("assistant reply"),
            "streamed response should be captured via sink without stdout noise"
        );

        server.abort();
    }

    #[tokio::test]
    #[serial]
    async fn prompt_with_model_sends_model_in_request() {
        let state = Arc::new(MockServerState::default());
        let app = mock_chat_router_capture_infer(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().unwrap();
        let make_svc = app.into_make_service();
        let server = tokio::spawn(async move {
            axum::serve(listener, make_svc)
                .await
                .expect("mock server run")
        });
        let base_url = format!("http://{}", addr);
        let config = test_config(&base_url);

        let sink = Arc::new(StdMutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Json, false, sink.clone());

        run_single_prompt(
            "test with model",
            None,
            Some("Qwen2.5-7B-Instruct".to_string()),
            64,
            0.5,
            config,
            false,
            &output,
        )
        .await
        .expect("prompt flow with model");

        // Verify model was sent in inference request
        let infer_requests = state.infer_requests.lock().await;
        let infer_body = infer_requests.last().expect("inference request");
        assert_eq!(
            infer_body["model"],
            Value::String("Qwen2.5-7B-Instruct".to_string())
        );

        server.abort();
    }

    #[tokio::test]
    #[serial]
    async fn cli_session_uses_source_type_and_lists_history() {
        let state = Arc::new(MockServerState::default());
        let app = mock_chat_router(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().unwrap();
        let make_svc = app.into_make_service();
        let server = tokio::spawn(async move {
            axum::serve(listener, make_svc)
                .await
                .expect("mock server run")
        });
        let base_url = format!("http://{}", addr);
        let config = test_config(&base_url);

        // Simulate auth store providing base URL (token unused by chat command)
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        let tmp_auth = NamedTempFile::new_in(&root).expect("tmp auth");
        env::set_var("AOSCTL_AUTH_PATH", tmp_auth.path());
        let auth = AuthStore {
            base_url: base_url.clone(),
            tenant_id: "tenant-1".to_string(),
            token: "test-token".to_string(),
            refresh_token: None,
            expires_at: None,
        };
        save_auth(&auth).expect("auth saved");

        let session_id = create_cli_session("cli", None, &config)
            .await
            .expect("cli session");
        add_cli_message(&session_id, "user", "hi", &config)
            .await
            .expect("user message");

        let output_sink = Arc::new(StdMutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Json, false, output_sink.clone());
        list_chat_sessions(true, &config, &output)
            .await
            .expect("list sessions");
        show_chat_history(&session_id, true, &config, &output)
            .await
            .expect("history");

        let sessions = state.sessions.lock().await;
        let session_body = sessions.last().expect("session body");
        assert_eq!(
            session_body["source_type"],
            Value::String("cli".to_string())
        );

        let messages = state.messages.lock().await;
        assert!(
            messages
                .iter()
                .any(|m| m["content"] == Value::String("hi".to_string())),
            "user message should be posted"
        );

        let captured_output = output_sink.lock().unwrap().join("\n");
        assert!(
            captured_output.contains("\"id\": \"session-mock\""),
            "session list output should be captured in sink"
        );
        assert!(
            captured_output.contains("\"content\": \"hi\""),
            "history output should include posted message without stdout noise"
        );

        server.abort();
        env::remove_var("AOSCTL_AUTH_PATH");
    }

    #[tokio::test]
    #[serial]
    async fn session_not_found_returns_helpful_error() {
        let _state = Arc::new(MockServerState::default());
        let app = mock_chat_router_with_errors();

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().unwrap();
        let make_svc = app.into_make_service();
        let server = tokio::spawn(async move {
            axum::serve(listener, make_svc)
                .await
                .expect("mock server run")
        });
        let base_url = format!("http://{}", addr);
        let config = test_config(&base_url);

        let output_sink = Arc::new(StdMutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Json, false, output_sink.clone());

        let result = show_chat_history("nonexistent-session", true, &config, &output).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not found") || err_msg.contains("404"),
            "Error should indicate session not found: {}",
            err_msg
        );

        server.abort();
    }

    #[tokio::test]
    #[serial]
    async fn connection_error_provides_helpful_message() {
        // Try to connect to a port that's not running
        let config = ChatConfig {
            base_url: "http://127.0.0.1:59999".to_string(),
            timeout: Duration::from_millis(100),
            retries: 1,
        };

        let output_sink = Arc::new(StdMutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Json, false, output_sink.clone());

        let result = list_chat_sessions(true, &config, &output).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Should contain helpful guidance
        assert!(
            err_msg.contains("connect") || err_msg.contains("server"),
            "Error should mention connection issue: {}",
            err_msg
        );
    }

    #[derive(Default)]
    struct MockServerState {
        sessions: Mutex<Vec<Value>>,
        messages: Mutex<Vec<Value>>,
        infer_requests: Mutex<Vec<Value>>,
    }

    fn mock_chat_router(state: Arc<MockServerState>) -> Router {
        Router::new()
            .route(
                "/v1/chat/sessions",
                post({
                    let state = state.clone();
                    move |Json(body): Json<Value>| {
                        let state = state.clone();
                        async move {
                            state.sessions.lock().await.push(body);
                            Json(serde_json::json!({ "session_id": "session-mock" }))
                        }
                    }
                })
                .get({
                    let state = state.clone();
                    move || {
                        let state = state.clone();
                        async move {
                            let sessions = state.sessions.lock().await;
                            Json(vec![serde_json::json!({
                                "id": "session-mock",
                                "name": "Mock Session",
                                "stack_id": null,
                                "source_type": sessions
                                    .last()
                                    .and_then(|s| s.get("source_type"))
                                    .cloned()
                                    .unwrap_or(Value::String("general".to_string())),
                                "created_at": "2024-01-01T00:00:00Z",
                                "last_activity_at": "2024-01-01T00:00:01Z"
                            })])
                        }
                    }
                }),
            )
            .route(
                "/v1/chat/sessions/{id}/messages",
                post({
                    let state = state.clone();
                    move |Path(_id): Path<String>, Json(body): Json<Value>| {
                        let state = state.clone();
                        async move {
                            state.messages.lock().await.push(body);
                            (
                                StatusCode::CREATED,
                                Json(serde_json::json!({
                                    "id": "msg-1",
                                    "session_id": "session-mock",
                                    "tenant_id": "tenant-1",
                                    "role": "user",
                                    "content": "ok",
                                    "timestamp": "2024-01-01T00:00:00Z",
                                    "created_at": "2024-01-01T00:00:00Z",
                                    "sequence": 0
                                })),
                            )
                        }
                    }
                })
                .get({
                    let state = state.clone();
                    move |Path(_id): Path<String>| {
                        let state = state.clone();
                        async move {
                            let messages = state.messages.lock().await;
                            let history: Vec<Value> = messages
                                .iter()
                                .enumerate()
                                .map(|(i, m)| {
                                    serde_json::json!({
                                        "id": format!("msg-{}", i),
                                        "session_id": "session-mock",
                                        "tenant_id": "tenant-1",
                                        "role": m["role"],
                                        "content": m["content"],
                                        "timestamp": "2024-01-01T00:00:00Z",
                                        "created_at": "2024-01-01T00:00:00Z",
                                        "sequence": i as i64
                                    })
                                })
                                .collect();
                            Json(history)
                        }
                    }
                }),
            )
            .route(
                "/v1/infer",
                post(|| async {
                    let body = "data: {\"id\":\"chunk-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"delta\":{\"content\":\"assistant reply\",\"role\":null},\"index\":0,\"finish_reason\":null}]}\n\ndata: [DONE]\n";
                    (StatusCode::OK, body)
                }),
            )
    }

    /// Mock router that captures inference requests for verification
    fn mock_chat_router_capture_infer(state: Arc<MockServerState>) -> Router {
        Router::new()
            .route(
                "/v1/chat/sessions",
                post({
                    let state = state.clone();
                    move |Json(body): Json<Value>| {
                        let state = state.clone();
                        async move {
                            state.sessions.lock().await.push(body);
                            Json(serde_json::json!({ "session_id": "session-mock" }))
                        }
                    }
                }),
            )
            .route(
                "/v1/chat/sessions/{id}/messages",
                post({
                    let state = state.clone();
                    move |Path(_id): Path<String>, Json(body): Json<Value>| {
                        let state = state.clone();
                        async move {
                            state.messages.lock().await.push(body);
                            (
                                StatusCode::CREATED,
                                Json(serde_json::json!({
                                    "id": "msg-1",
                                    "session_id": "session-mock",
                                    "tenant_id": "tenant-1",
                                    "role": "user",
                                    "content": "ok",
                                    "timestamp": "2024-01-01T00:00:00Z",
                                    "created_at": "2024-01-01T00:00:00Z",
                                    "sequence": 0
                                })),
                            )
                        }
                    }
                }),
            )
            .route(
                "/v1/infer",
                post({
                    let state = state.clone();
                    move |Json(body): Json<Value>| {
                        let state = state.clone();
                        async move {
                            state.infer_requests.lock().await.push(body);
                            let response_body = "data: {\"id\":\"chunk-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"delta\":{\"content\":\"assistant reply\",\"role\":null},\"index\":0,\"finish_reason\":null}]}\n\ndata: [DONE]\n";
                            (StatusCode::OK, response_body)
                        }
                    }
                }),
            )
    }

    /// Mock router that returns errors for testing error handling
    fn mock_chat_router_with_errors() -> Router {
        Router::new().route(
            "/v1/chat/sessions/{id}/messages",
            get(|Path(_id): Path<String>| async {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Session not found" })),
                )
            }),
        )
    }
}
