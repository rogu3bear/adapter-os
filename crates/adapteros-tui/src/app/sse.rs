//! Server-Sent Events (SSE) client for streaming inference and training progress
//!
//! Provides async streaming for real-time updates from the server.

use anyhow::Result;
use futures_util::StreamExt;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

/// Token received from inference stream
#[derive(Debug, Clone, Deserialize)]
pub struct StreamToken {
    pub token: String,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// SSE Client for streaming operations
pub struct SseClient {
    base_url: String,
    client: reqwest::Client,
}

impl SseClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Stream inference tokens
    /// Returns a channel receiver that yields tokens as they arrive
    pub async fn stream_inference(
        &self,
        prompt: &str,
        adapter_id: Option<&str>,
    ) -> Result<mpsc::Receiver<StreamToken>> {
        let url = format!("{}/v1/infer/stream", self.base_url);

        let mut body = serde_json::json!({
            "prompt": prompt,
            "max_tokens": 512,
            "temperature": 0.7,
            "stream": true,
        });

        if let Some(adapter) = adapter_id {
            body["adapter_id"] = serde_json::json!(adapter);
        }

        let (tx, rx) = mpsc::channel(100);

        let request = self.client.post(&url).json(&body);

        let mut es = request.eventsource()?;

        tokio::spawn(async move {
            while let Some(event) = es.next().await {
                match event {
                    Ok(Event::Open) => {
                        debug!("SSE connection opened for inference");
                    }
                    Ok(Event::Message(msg)) => {
                        if msg.data == "[DONE]" {
                            debug!("Inference stream complete");
                            break;
                        }

                        match serde_json::from_str::<StreamToken>(&msg.data) {
                            Ok(token) => {
                                if tx.send(token.clone()).await.is_err() {
                                    debug!("Receiver dropped, stopping stream");
                                    break;
                                }

                                if token.finish_reason.is_some() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse SSE token: {} (data: {})", e, msg.data);
                            }
                        }
                    }
                    Err(e) => {
                        error!("SSE error: {}", e);
                        break;
                    }
                }
            }
            es.close();
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_token_deserialize() {
        let json = r#"{"token": "Hello", "finish_reason": null}"#;
        let token: StreamToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.token, "Hello");
        assert!(token.finish_reason.is_none());
    }

    #[test]
    fn test_stream_token_with_finish_reason() {
        let json = r#"{"token": ".", "finish_reason": "stop"}"#;
        let token: StreamToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.token, ".");
        assert_eq!(token.finish_reason, Some("stop".to_string()));
    }
}
