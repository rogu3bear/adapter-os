//! Interactive chat CLI with streaming support

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use chrono::{SecondsFormat, Utc};
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

        /// Mark session as owned by system assistant (source_type=owner_system)
        #[arg(long)]
        owner_system: bool,
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
            owner_system,
        } => run_interactive_chat(stack, &base_url, verbose, owner_system, output).await,
        ChatCommand::Prompt {
            text,
            stack,
            max_tokens,
            temperature,
            base_url,
            owner_system,
        } => {
            run_single_prompt(
                &text,
                stack,
                max_tokens,
                temperature,
                &base_url,
                owner_system,
                output,
            )
            .await
        }
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
    owner_system: bool,
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
    let session_source = if owner_system { "owner_system" } else { "cli" };
    let session_id = create_cli_session(session_source, current_stack.clone(), base_url).await?;

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
                        output.info(format!("Switched to stack: {}", stack_id));
                    }
                    continue;
                }

                // Send inference request
                match run_cli_turn(&session_id, input, current_stack.clone(), base_url, verbose)
                    .await
                {
                    Ok(_) => output.blank(),
                    Err(e) => {
                        error!(error = %e, "Inference failed");
                        output.error(format!("Error: {}", e));
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
    output: Option<&OutputWriter>,
    max_tokens: Option<usize>,
    temperature: Option<f32>,
) -> Result<String> {
    let request = InferenceRequest {
        prompt: prompt.to_string(),
        max_tokens,
        temperature,
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
        return Err(AosError::Http(format!(
            "Inference failed: {} {}",
            status, text
        )));
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
                        eprintln!("\nWarning: Failed to parse chunk: {}", e);
                    }
                }
            }
        }
    }

    if let Some(out) = output {
        out.result(&full_text);
    }

    Ok(full_text)
}

/// Run single prompt (non-interactive)
async fn run_single_prompt(
    text: &str,
    stack: Option<String>,
    max_tokens: usize,
    temperature: f32,
    base_url: &str,
    owner_system: bool,
    output: &OutputWriter,
) -> Result<()> {
    info!(prompt = %text, stack = ?stack, "Running single prompt");

    let source_type = if owner_system {
        "owner_system"
    } else {
        "cli_prompt"
    };
    let session_id = create_cli_session(source_type, stack.clone(), base_url).await?;
    add_cli_message(&session_id, "user", text, base_url).await?;
    let response_text = send_streaming_inference(
        text,
        stack.clone(),
        base_url,
        false,
        Some(output),
        Some(max_tokens),
        Some(temperature),
    )
    .await?;
    add_cli_message(&session_id, "assistant", &response_text, base_url).await?;
    output.blank();

    Ok(())
}

/// List saved chat sessions
async fn list_chat_sessions(json: bool, base_url: &str, output: &OutputWriter) -> Result<()> {
    info!("Listing chat sessions");

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
        return Err(AosError::Http(format!(
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
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    info!(session_id = %session_id, "Showing chat history");

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/chat/sessions/{}/messages",
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
        return Err(AosError::Http(format!(
            "Failed to get history: {} {}",
            status, text
        )));
    }

    let history: Vec<ChatMessageResponse> = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

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
    base_url: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/sessions", base_url.trim_end_matches('/'));
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
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "Failed to create chat session: {} {}",
            status, text
        )));
    }

    let created: CreateSessionResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;
    Ok(created.session_id)
}

async fn add_cli_message(
    session_id: &str,
    role: &str,
    content: &str,
    base_url: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/chat/sessions/{}/messages",
        base_url.trim_end_matches('/'),
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
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "Failed to add message: {} {}",
            status, text
        )));
    }

    Ok(())
}

async fn run_cli_turn(
    session_id: &str,
    user_input: &str,
    stack: Option<String>,
    base_url: &str,
    verbose: bool,
) -> Result<()> {
    add_cli_message(session_id, "user", user_input, base_url).await?;
    let assistant_text = send_streaming_inference(
        user_input,
        stack.clone(),
        base_url,
        verbose,
        None,
        Some(500),
        Some(0.7),
    )
    .await?;
    add_cli_message(session_id, "assistant", &assistant_text, base_url).await?;
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

    #[test]
    fn test_get_chat_command_name() {
        assert_eq!(
            get_chat_command_name(&ChatCommand::Interactive {
                stack: None,
                base_url: "http://localhost:8080".to_string(),
                verbose: false,
                owner_system: false,
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
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: InferenceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.prompt, deserialized.prompt);
        assert_eq!(req.max_tokens, deserialized.max_tokens);
        assert_eq!(req.stream, deserialized.stream);
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

        let sink = Arc::new(StdMutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Json, false, sink.clone());

        run_single_prompt("hello cli", None, 64, 0.5, &base_url, false, &output)
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

        let session_id = create_cli_session("cli", None, &base_url)
            .await
            .expect("cli session");
        add_cli_message(&session_id, "user", "hi", &base_url)
            .await
            .expect("user message");

        let output_sink = Arc::new(StdMutex::new(Vec::new()));
        let output = OutputWriter::with_sink(OutputMode::Json, false, output_sink.clone());
        list_chat_sessions(true, &base_url, &output)
            .await
            .expect("list sessions");
        show_chat_history(&session_id, true, &base_url, &output)
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

    #[derive(Default)]
    struct MockServerState {
        sessions: Mutex<Vec<Value>>,
        messages: Mutex<Vec<Value>>,
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
}
