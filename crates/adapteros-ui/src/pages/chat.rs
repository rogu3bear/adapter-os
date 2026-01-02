//! Chat page with SSE streaming support
//!
//! This module provides the chat interface with real-time token streaming
//! using Server-Sent Events (SSE) for inference responses.

use crate::api::api_base_url;
use crate::components::{Button, Card, Shell, Spinner, Textarea, TraceButton, TracePanel};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

/// Streaming inference request for POST /v1/infer/stream
#[derive(Debug, Clone, Serialize)]
struct StreamingInferRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
}

/// SSE event types from the streaming inference endpoint
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
enum InferenceEvent {
    /// Inference token
    Token {
        text: String,
    },
    /// Inference complete
    Done {
        #[serde(default)]
        total_tokens: usize,
        #[serde(default)]
        latency_ms: u64,
        #[serde(default)]
        trace_id: Option<String>,
    },
    /// Error occurred
    Error {
        message: String,
    },
    /// Catch-all for other events (Loading, Ready, etc.)
    #[serde(other)]
    Other,
}

/// OpenAI-compatible streaming chunk (alternative format)
#[derive(Debug, Clone, Deserialize)]
struct StreamingChunk {
    #[serde(default)]
    pub choices: Vec<StreamingChoice>,
}

#[derive(Debug, Clone, Deserialize)]
struct StreamingChoice {
    #[serde(default)]
    pub delta: Delta,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Delta {
    #[serde(default)]
    pub content: Option<String>,
}

/// Chat sessions list page
#[component]
pub fn Chat() -> impl IntoView {
    // Create a new session ID when clicking "New Session"
    let create_session = move |_| {
        let session_id = format!("session-{}", js_sys::Date::now() as u64);
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href(&format!("/chat/{}", session_id));
        }
    };

    view! {
        <Shell>
            <div class="space-y-6">
                <div class="flex items-center justify-between">
                    <h1 class="text-3xl font-bold tracking-tight">"Chat"</h1>
                    <Button on_click=Callback::new(create_session)>
                        "New Session"
                    </Button>
                </div>

                <Card>
                    <div class="py-8 text-center">
                        <p class="text-muted-foreground">"Select or create a chat session to get started"</p>
                    </div>
                </Card>
            </div>
        </Shell>
    }
}

/// Chat session page with SSE streaming
#[component]
pub fn ChatSession() -> impl IntoView {
    let params = use_params_map();
    let session_id = move || params.get().get("session_id").unwrap_or_default();

    let message = RwSignal::new(String::new());
    let messages: RwSignal<Vec<ChatMessage>> = RwSignal::new(vec![]);
    let loading = RwSignal::new(false);
    let streaming = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let selected_trace = RwSignal::new(Option::<String>::None);
    let show_trace_panel = RwSignal::new(false);

    // Use a Callback for the send action with SSE streaming
    let do_send = Callback::new(move |_: ()| {
        let msg = message.get();
        if msg.trim().is_empty() {
            return;
        }

        // Add user message
        messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: "user".to_string(),
                content: msg.clone(),
                is_streaming: false,
                trace_id: None,
                latency_ms: None,
                token_count: None,
            });
        });

        message.set(String::new());
        loading.set(true);
        streaming.set(true);
        error.set(None);

        // Build conversation context
        let conversation = messages.get();
        let prompt = conversation
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        // Add a placeholder assistant message for streaming
        messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
                is_streaming: true,
                trace_id: None,
                latency_ms: None,
                token_count: None,
            });
        });

        // Get the auth token from localStorage
        let auth_token = get_auth_token();

        wasm_bindgen_futures::spawn_local(async move {
            let request = StreamingInferRequest {
                prompt,
                max_tokens: Some(1024),
                temperature: Some(0.7),
                adapters: None,
            };

            match stream_inference(&request, auth_token.as_deref(), messages, streaming).await {
                Ok(trace_info) => {
                    // Mark the last message as no longer streaming and add trace info
                    messages.update(|msgs| {
                        if let Some(last) = msgs.last_mut() {
                            if last.role == "assistant" {
                                last.is_streaming = false;
                                last.trace_id = trace_info.trace_id;
                                last.latency_ms = trace_info.latency_ms;
                                last.token_count = trace_info.token_count;
                            }
                        }
                    });
                }
                Err(e) => {
                    // Remove the empty assistant message on error
                    messages.update(|msgs| {
                        if let Some(last) = msgs.last() {
                            if last.role == "assistant" && last.content.is_empty() {
                                msgs.pop();
                            }
                        }
                    });
                    error.set(Some(e));
                }
            }

            loading.set(false);
            streaming.set(false);
        });
    });

    view! {
        <Shell>
            <div class="flex h-[calc(100vh-8rem)] flex-col">
                // Header
                <div class="flex items-center justify-between border-b pb-4">
                    <h1 class="text-xl font-semibold">"Chat Session"</h1>
                    <div class="flex items-center gap-2">
                        {move || {
                            if streaming.get() {
                                view! {
                                    <span class="text-xs text-green-500 animate-pulse">"Streaming..."</span>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                        <span class="text-sm text-muted-foreground">{session_id}</span>
                    </div>
                </div>

                // Messages
                <div class="flex-1 overflow-y-auto py-4">
                    {move || {
                        let msgs = messages.get();
                        if msgs.is_empty() {
                            view! {
                                <div class="flex h-full items-center justify-center">
                                    <p class="text-muted-foreground">"No messages yet. Start the conversation!"</p>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-4">
                                    {msgs
                                        .into_iter()
                                        .map(|msg| {
                                            let is_user = msg.role == "user";
                                            let is_streaming = msg.is_streaming;
                                            let trace_id = msg.trace_id.clone();
                                            let latency_ms = msg.latency_ms;
                                            let token_count = msg.token_count;
                                            view! {
                                                <div class=format!(
                                                    "flex {}",
                                                    if is_user { "justify-end" } else { "justify-start" }
                                                )>
                                                    <div class="flex flex-col gap-1 max-w-[80%]">
                                                        <div class=format!(
                                                            "rounded-lg px-4 py-2 {}",
                                                            if is_user {
                                                                "bg-primary text-primary-foreground"
                                                            } else {
                                                                "bg-muted"
                                                            }
                                                        )>
                                                            <p class="whitespace-pre-wrap">
                                                                {msg.content.clone()}
                                                                {if is_streaming && !msg.content.is_empty() {
                                                                    view! { <span class="animate-pulse">"_"</span> }.into_any()
                                                                } else if is_streaming {
                                                                    view! { <Spinner/> }.into_any()
                                                                } else {
                                                                    view! { <span></span> }.into_any()
                                                                }}
                                                            </p>
                                                        </div>
                                                        // Show trace button for assistant messages with trace info
                                                        {if !is_user && !is_streaming && trace_id.is_some() {
                                                            let tid = trace_id.clone().unwrap();
                                                            let latency = latency_ms.unwrap_or(0);
                                                            Some(view! {
                                                                <div class="flex items-center gap-2 pl-1">
                                                                    <TraceButton
                                                                        trace_id=tid.clone()
                                                                        latency_ms=latency
                                                                        on_click=Callback::new(move |id: String| {
                                                                            selected_trace.set(Some(id));
                                                                            show_trace_panel.set(true);
                                                                        })
                                                                    />
                                                                    {token_count.map(|tc| view! {
                                                                        <span class="text-xs text-muted-foreground">
                                                                            {tc}" tokens"
                                                                        </span>
                                                                    })}
                                                                </div>
                                                            })
                                                        } else {
                                                            None
                                                        }}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }
                    }}
                </div>

                // Trace panel (modal overlay)
                {move || {
                    if show_trace_panel.get() {
                        selected_trace.get().map(|tid| {
                            view! {
                                <TracePanel
                                    trace_id=tid.clone()
                                    on_close=Callback::new(move |_| {
                                        show_trace_panel.set(false);
                                        selected_trace.set(None);
                                    })
                                />
                            }
                        })
                    } else {
                        None
                    }
                }}

                // Error display
                {move || {
                    error.get().map(|e| view! {
                        <div class="rounded-md bg-destructive/10 border border-destructive p-3 mb-4">
                            <p class="text-sm text-destructive">{e}</p>
                        </div>
                    })
                }}

                // Input
                <div class="border-t pt-4">
                    <form
                        class="flex gap-4"
                        on:submit=move |ev: web_sys::SubmitEvent| {
                            ev.prevent_default();
                            if !loading.get() {
                                do_send.run(());
                            }
                        }
                    >
                        <Textarea
                            value=message
                            placeholder="Type your message...".to_string()
                            class="flex-1".to_string()
                            rows=2
                        />
                        <Button
                            loading=loading.get()
                            on_click=do_send
                        >
                            "Send"
                        </Button>
                    </form>
                </div>
            </div>
        </Shell>
    }
}

#[derive(Debug, Clone)]
struct ChatMessage {
    role: String,
    content: String,
    is_streaming: bool,
    /// Trace ID for this message (if available from inference)
    trace_id: Option<String>,
    /// Latency in milliseconds (if available)
    latency_ms: Option<u64>,
    /// Total tokens generated
    token_count: Option<u32>,
}

/// Get auth token from localStorage
fn get_auth_token() -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("auth_token").ok().flatten())
}

/// Trace info returned from stream_inference
#[derive(Debug, Clone, Default)]
struct StreamTraceInfo {
    trace_id: Option<String>,
    latency_ms: Option<u64>,
    token_count: Option<u32>,
}

/// Stream inference using POST SSE endpoint
///
/// This function connects to the streaming inference endpoint and
/// accumulates tokens into the assistant message in real-time.
async fn stream_inference(
    request: &StreamingInferRequest,
    auth_token: Option<&str>,
    messages: RwSignal<Vec<ChatMessage>>,
    streaming: RwSignal<bool>,
) -> Result<StreamTraceInfo, String> {
    let url = format!("{}/v1/infer/stream", api_base_url());

    let body = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;

    // Create fetch request with POST method
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&JsValue::from_str(&body));

    let request_obj = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    // Set headers
    request_obj
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set Content-Type header: {:?}", e))?;

    request_obj
        .headers()
        .set("Accept", "text/event-stream")
        .map_err(|e| format!("Failed to set Accept header: {:?}", e))?;

    if let Some(token) = auth_token {
        request_obj
            .headers()
            .set("Authorization", &format!("Bearer {}", token))
            .map_err(|e| format!("Failed to set Authorization header: {:?}", e))?;
    }

    // Fetch the response
    let window = web_sys::window().ok_or("No window object")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request_obj))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?
        .dyn_into()
        .map_err(|_| "Response is not a Response object")?;

    if !response.ok() {
        let status = response.status();
        let status_text = response.status_text();
        return Err(format!("HTTP error {}: {}", status, status_text));
    }

    // Get the response body as a ReadableStream
    let body_stream = response
        .body()
        .ok_or("No response body")?;

    // Get the reader from the stream
    let reader = body_stream
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| "Failed to get reader")?;

    // Buffer for incomplete SSE data
    let mut buffer = String::new();
    let mut trace_info = StreamTraceInfo::default();

    // Read and process chunks
    loop {
        // Check if streaming was cancelled
        if !streaming.get() {
            break;
        }

        let result = JsFuture::from(reader.read())
            .await
            .map_err(|e| format!("Read failed: {:?}", e))?;

        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
            .map_err(|_| "Failed to get done property")?
            .as_bool()
            .unwrap_or(true);

        if done {
            break;
        }

        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
            .map_err(|_| "Failed to get value property")?;

        if value.is_undefined() {
            continue;
        }

        // Convert Uint8Array to string
        let array = js_sys::Uint8Array::new(&value);
        let bytes: Vec<u8> = array.to_vec();
        let chunk = String::from_utf8_lossy(&bytes).to_string();

        buffer.push_str(&chunk);

        // Process complete SSE events from buffer
        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            buffer = buffer[event_end + 2..].to_string();

            // Parse SSE event
            let parsed = parse_sse_event_with_info(&event_data);
            if let Some(token_content) = parsed.token {
                // Append token to the last (assistant) message
                messages.update(|msgs| {
                    if let Some(last) = msgs.last_mut() {
                        if last.role == "assistant" {
                            last.content.push_str(&token_content);
                        }
                    }
                });
            }
            // Capture trace info from Done event
            if parsed.trace_id.is_some() {
                trace_info.trace_id = parsed.trace_id;
            }
            if parsed.latency_ms.is_some() {
                trace_info.latency_ms = parsed.latency_ms;
            }
            if parsed.token_count.is_some() {
                trace_info.token_count = parsed.token_count;
            }
        }
    }

    Ok(trace_info)
}

/// Parsed SSE event result
#[derive(Debug, Clone, Default)]
struct ParsedSseEvent {
    token: Option<String>,
    trace_id: Option<String>,
    latency_ms: Option<u64>,
    token_count: Option<u32>,
}

/// Parse an SSE event and extract token content plus trace info
fn parse_sse_event_with_info(event_data: &str) -> ParsedSseEvent {
    let mut result = ParsedSseEvent::default();

    // SSE events have format:
    // event: <event_type>
    // data: <json_data>
    // or just:
    // data: <json_data>

    let mut data_line: Option<&str> = None;

    for line in event_data.lines() {
        if line.starts_with("data: ") {
            data_line = Some(&line[6..]);
        }
    }

    let data = match data_line {
        Some(d) => d,
        None => return result,
    };

    // Check for [DONE] marker
    if data == "[DONE]" {
        return result;
    }

    // Try parsing as InferenceEvent first (AdapterOS format)
    if let Ok(event) = serde_json::from_str::<InferenceEvent>(data) {
        match event {
            InferenceEvent::Token { text } => {
                result.token = Some(text);
            }
            InferenceEvent::Done { total_tokens, latency_ms, trace_id } => {
                result.trace_id = trace_id;
                result.latency_ms = Some(latency_ms);
                result.token_count = Some(total_tokens as u32);
            }
            InferenceEvent::Error { message } => {
                // Log error but don't return it as content
                web_sys::console::error_1(&JsValue::from_str(&format!("Stream error: {}", message)));
            }
            InferenceEvent::Other => {
                // Ignore Loading, Ready, and other unhandled events
            }
        }
        return result;
    }

    // Try parsing as OpenAI-compatible StreamingChunk
    if let Ok(chunk) = serde_json::from_str::<StreamingChunk>(data) {
        if let Some(choice) = chunk.choices.first() {
            if let Some(content) = &choice.delta.content {
                result.token = Some(content.clone());
            }
        }
    }

    result
}
