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

/// Training progress event
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TrainingProgressEvent {
    pub job_id: String,
    #[serde(default)]
    pub progress_pct: f32,
    #[serde(default)]
    pub current_epoch: u32,
    #[serde(default)]
    pub current_loss: f32,
    #[serde(default)]
    pub tokens_per_second: f32,
    #[serde(default)]
    pub event_type: String,
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

    /// Stream training job progress
    #[allow(dead_code)]
    pub async fn stream_training_progress(
        &self,
        job_id: &str,
    ) -> Result<mpsc::Receiver<TrainingProgressEvent>> {
        let url = format!("{}/v1/training/{}/progress", self.base_url, job_id);

        let (tx, rx) = mpsc::channel(100);

        let request = self.client.get(&url);
        let mut es = request.eventsource()?;

        let job_id_owned = job_id.to_string();

        tokio::spawn(async move {
            while let Some(event) = es.next().await {
                match event {
                    Ok(Event::Open) => {
                        debug!("SSE connection opened for training progress");
                    }
                    Ok(Event::Message(msg)) => {
                        if msg.data == "[DONE]" {
                            debug!("Training stream complete");
                            break;
                        }

                        match serde_json::from_str::<TrainingProgressEvent>(&msg.data) {
                            Ok(mut progress) => {
                                progress.job_id = job_id_owned.clone();
                                if tx.send(progress).await.is_err() {
                                    debug!("Receiver dropped, stopping stream");
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse training progress: {}", e);
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

    #[test]
    fn test_training_progress_deserialize() {
        let json = r#"{"job_id": "test", "progress_pct": 50.0, "current_loss": 0.25}"#;
        let progress: TrainingProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(progress.progress_pct, 50.0);
        assert_eq!(progress.current_loss, 0.25);
    }

    #[test]
    fn test_training_progress_defaults() {
        let json = r#"{"job_id": "test-job"}"#;
        let progress: TrainingProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(progress.job_id, "test-job");
        assert_eq!(progress.progress_pct, 0.0);
        assert_eq!(progress.current_epoch, 0);
        assert_eq!(progress.current_loss, 0.0);
        assert_eq!(progress.tokens_per_second, 0.0);
        assert_eq!(progress.event_type, "");
    }
}
