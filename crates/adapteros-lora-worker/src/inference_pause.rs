//! Inference Pause/Resume Management
//!
//! Provides the ability to pause inference when external review is needed,
//! and resume when review is provided.
//!
//! # Flow
//!
//! 1. Inference detects need for review (via reasoning router or explicit tag)
//! 2. Creates a `PausedInference` and registers with `InferencePauseRegistry`
//! 3. Waits on the pause token for review
//! 4. Human submits review via API
//! 5. Registry resumes the paused inference with review context
//! 6. Inference continues with review incorporated
#![allow(clippy::let_underscore_future)]

use std::collections::HashMap;
use std::time::Instant;

use parking_lot::RwLock;
use tokio::sync::oneshot;

use adapteros_api_types::review::{
    InferenceState, PauseKind, PauseReason, Review, ReviewContext, ReviewScope, SubmitReviewRequest,
};
use adapteros_core::{AosError, Result};
use adapteros_id::{TypedId, IdPrefix};

// =============================================================================
// Pause Token
// =============================================================================

/// Token representing a paused inference, allowing it to wait for review
pub struct InferencePauseToken {
    /// Unique pause ID
    pub pause_id: String,
    /// Inference request ID
    pub inference_id: String,
    /// Why it was paused
    pub reason: PauseReason,
    /// When it was paused
    pub paused_at: Instant,
    /// Sender to provide the review (held by registry)
    resume_tx: Option<oneshot::Sender<Review>>,
    /// Receiver to wait for review (held by inference task)
    resume_rx: Option<oneshot::Receiver<Review>>,
}

impl InferencePauseToken {
    /// Create a new pause token
    pub fn new(inference_id: String, reason: PauseReason) -> Self {
        let (tx, rx) = oneshot::channel();
        Self {
            pause_id: reason.pause_id.clone(),
            inference_id,
            reason,
            paused_at: Instant::now(),
            resume_tx: Some(tx),
            resume_rx: Some(rx),
        }
    }

    /// Get duration paused in seconds
    pub fn duration_secs(&self) -> u64 {
        self.paused_at.elapsed().as_secs()
    }

    /// Take the receiver (for the inference task to wait on)
    pub fn take_receiver(&mut self) -> Option<oneshot::Receiver<Review>> {
        self.resume_rx.take()
    }

    /// Take the sender (for the registry to resume with)
    pub fn take_sender(&mut self) -> Option<oneshot::Sender<Review>> {
        self.resume_tx.take()
    }
}

// =============================================================================
// Pause Registry
// =============================================================================

/// Registry of all paused inferences
#[derive(Default)]
pub struct InferencePauseRegistry {
    /// Map of pause_id -> paused inference info
    paused: RwLock<HashMap<String, PausedEntry>>,
}

/// Entry in the pause registry
struct PausedEntry {
    /// Inference request ID
    inference_id: String,
    /// Pause reason
    reason: PauseReason,
    /// When paused
    paused_at: Instant,
    /// Sender to provide review (triggers resume)
    resume_tx: oneshot::Sender<Review>,
}

/// Info about a paused inference (for API responses)
#[derive(Debug, Clone)]
pub struct PausedInferenceInfo {
    pub inference_id: String,
    pub pause_id: String,
    pub kind: PauseKind,
    pub context: ReviewContext,
    pub paused_at: Instant,
    pub duration_secs: u64,
}

impl InferencePauseRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            paused: RwLock::new(HashMap::new()),
        }
    }

    /// Register a paused inference, returns receiver for the inference to wait on
    ///
    /// # Errors
    /// Returns an error if the token's sender or receiver has already been taken.
    pub fn register(&self, mut token: InferencePauseToken) -> Result<oneshot::Receiver<Review>> {
        let pause_id = token.pause_id.clone();
        let inference_id = token.inference_id.clone();
        let reason = token.reason.clone();
        let paused_at = token.paused_at;

        // Take sender and receiver from token
        let resume_tx = token
            .take_sender()
            .ok_or_else(|| AosError::internal("InferencePauseToken sender already taken"))?;
        let resume_rx = token
            .take_receiver()
            .ok_or_else(|| AosError::internal("InferencePauseToken receiver already taken"))?;

        let entry = PausedEntry {
            inference_id,
            reason,
            paused_at,
            resume_tx,
        };

        self.paused.write().insert(pause_id, entry);
        Ok(resume_rx)
    }

    /// Submit a review and resume the paused inference
    pub fn submit_review(&self, request: SubmitReviewRequest) -> Result<InferenceState> {
        let mut guard = self.paused.write();

        let entry = guard.remove(&request.pause_id).ok_or_else(|| {
            AosError::validation(format!(
                "No paused inference found with pause_id: {}",
                request.pause_id
            ))
        })?;

        // Send the review to resume the waiting inference
        entry
            .resume_tx
            .send(request.review)
            .map_err(|_| AosError::internal("Inference task no longer waiting for review"))?;

        Ok(InferenceState::Running)
    }

    /// Check if a pause_id is registered
    pub fn is_paused(&self, pause_id: &str) -> bool {
        self.paused.read().contains_key(pause_id)
    }

    /// Get state for a specific inference (by inference_id)
    pub fn get_state_by_inference(&self, inference_id: &str) -> Option<PausedInferenceInfo> {
        let guard = self.paused.read();
        for (pause_id, entry) in guard.iter() {
            if entry.inference_id == inference_id {
                return Some(PausedInferenceInfo {
                    inference_id: entry.inference_id.clone(),
                    pause_id: pause_id.clone(),
                    kind: entry.reason.kind.clone(),
                    context: entry.reason.context.clone(),
                    paused_at: entry.paused_at,
                    duration_secs: entry.paused_at.elapsed().as_secs(),
                });
            }
        }
        None
    }

    /// Get state for a specific pause (by pause_id)
    pub fn get_state_by_pause(&self, pause_id: &str) -> Option<PausedInferenceInfo> {
        let guard = self.paused.read();
        guard.get(pause_id).map(|entry| PausedInferenceInfo {
            inference_id: entry.inference_id.clone(),
            pause_id: pause_id.to_string(),
            kind: entry.reason.kind.clone(),
            context: entry.reason.context.clone(),
            paused_at: entry.paused_at,
            duration_secs: entry.paused_at.elapsed().as_secs(),
        })
    }

    /// List all paused inferences
    pub fn list_paused(&self) -> Vec<PausedInferenceInfo> {
        let guard = self.paused.read();
        guard
            .iter()
            .map(|(pause_id, entry)| PausedInferenceInfo {
                inference_id: entry.inference_id.clone(),
                pause_id: pause_id.clone(),
                kind: entry.reason.kind.clone(),
                context: entry.reason.context.clone(),
                paused_at: entry.paused_at,
                duration_secs: entry.paused_at.elapsed().as_secs(),
            })
            .collect()
    }

    /// Remove a paused inference without resuming (e.g., on timeout or cancel)
    pub fn unregister(&self, pause_id: &str) -> bool {
        self.paused.write().remove(pause_id).is_some()
    }

    /// Get count of paused inferences
    pub fn count(&self) -> usize {
        self.paused.read().len()
    }
}

// =============================================================================
// Pause Handle (for inference tasks)
// =============================================================================

/// Handle given to inference task to wait for review
pub struct InferencePauseHandle {
    /// Receiver to wait for review
    rx: oneshot::Receiver<Review>,
    /// Pause ID (for logging/tracking)
    pub pause_id: String,
    /// Context that was sent for review
    pub context: ReviewContext,
}

impl InferencePauseHandle {
    /// Create from registry registration
    pub fn new(rx: oneshot::Receiver<Review>, pause_id: String, context: ReviewContext) -> Self {
        Self {
            rx,
            pause_id,
            context,
        }
    }

    /// Wait for review (blocks until review is submitted)
    pub async fn wait_for_review(self) -> Result<Review> {
        self.rx.await.map_err(|_| {
            AosError::internal("Review channel closed - pause was cancelled or timed out")
        })
    }

    /// Wait for review with timeout
    pub async fn wait_for_review_timeout(
        self,
        timeout: std::time::Duration,
    ) -> Result<Option<Review>> {
        match tokio::time::timeout(timeout, self.rx).await {
            Ok(Ok(review)) => Ok(Some(review)),
            Ok(Err(_)) => Err(AosError::internal("Review channel closed")),
            Err(_) => Ok(None), // Timeout, no review received
        }
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Create a pause for code review
pub fn pause_for_code_review(
    inference_id: &str,
    code: &str,
    question: &str,
) -> (InferencePauseToken, ReviewContext) {
    let pause_id = TypedId::new(IdPrefix::Rvw).to_string();
    let context = ReviewContext::code_review(code, question);
    let reason = PauseReason::review_needed(&pause_id, context.clone());

    let token = InferencePauseToken::new(inference_id.to_string(), reason);
    (token, context)
}

/// Create a pause for high-severity threat escalation requiring human review
pub fn pause_for_threat_escalation(
    inference_id: &str,
    threat_summary: &str,
    severity: &str,
    evidence: Option<serde_json::Value>,
) -> (InferencePauseToken, ReviewContext) {
    let pause_id = TypedId::new(IdPrefix::Inc).to_string();
    let question = format!(
        "High-severity threat detected ({}). Human review required before inference can continue.",
        severity
    );
    let mut context = ReviewContext {
        code: Some(threat_summary.to_string()),
        question: Some(question),
        scope: vec![ReviewScope::Security],
        metadata: evidence,
    };
    // Add security scope for threat-related reviews
    context.scope = vec![ReviewScope::Security];

    let reason = PauseReason::threat_escalation(&pause_id, context.clone());
    let token = InferencePauseToken::new(inference_id.to_string(), reason);
    (token, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_api_types::review::{Review, ReviewAssessment};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_pause_resume_flow() {
        let registry = Arc::new(InferencePauseRegistry::new());

        // Create pause
        let (token, context) = pause_for_code_review("infer-1", "fn foo() {}", "Is this correct?");
        let pause_id = token.pause_id.clone();

        // Register and get receiver
        let rx = registry
            .register(token)
            .expect("Test register should succeed");
        let handle = InferencePauseHandle::new(rx, pause_id.clone(), context);

        // Verify it's paused
        assert!(registry.is_paused(&pause_id));
        assert_eq!(registry.count(), 1);

        // Submit review in background
        let registry_clone = registry.clone();
        let pause_id_clone = pause_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let request = SubmitReviewRequest {
                pause_id: pause_id_clone,
                review: Review::approved(Some("Looks good!".into())),
                reviewer: "test".into(),
            };
            registry_clone.submit_review(request).unwrap();
        });

        // Wait for review
        let review = handle.wait_for_review().await.unwrap();
        assert_eq!(review.assessment, ReviewAssessment::Approved);

        // Verify it's no longer paused
        assert!(!registry.is_paused(&pause_id));
    }

    #[tokio::test]
    async fn test_list_paused() {
        let registry = InferencePauseRegistry::new();

        let (token1, _) = pause_for_code_review("infer-1", "code1", "q1");
        let (token2, _) = pause_for_code_review("infer-2", "code2", "q2");

        let _ = registry
            .register(token1)
            .expect("Test register should succeed");
        let _ = registry
            .register(token2)
            .expect("Test register should succeed");

        let paused = registry.list_paused();
        assert_eq!(paused.len(), 2);
    }
}
