//! Reviews API client
//!
//! Client functions for the human-in-the-loop review queue.

use super::{ApiClient, ApiResult};
use adapteros_api_types::review::{
    InferenceStateResponse, ListPausedResponse, ReviewContextExport, SubmitReviewRequest,
    SubmitReviewResponse,
};

impl ApiClient {
    /// Fetch all paused reviews from the queue
    pub async fn list_paused_reviews(&self) -> ApiResult<ListPausedResponse> {
        self.get("/v1/reviews/paused").await
    }

    /// Fetch pause details for a specific pause ID.
    pub async fn get_pause_details(&self, pause_id: &str) -> ApiResult<InferenceStateResponse> {
        self.get(&format!("/v1/reviews/{}", pause_id)).await
    }

    /// Export review context for external reviewers (JSON bundle).
    pub async fn export_review_context(&self, pause_id: &str) -> ApiResult<ReviewContextExport> {
        self.get(&format!("/v1/reviews/{}/context", pause_id)).await
    }

    /// Submit a review for a paused inference
    pub async fn submit_review(
        &self,
        request: &SubmitReviewRequest,
    ) -> ApiResult<SubmitReviewResponse> {
        self.post("/v1/reviews/submit", request).await
    }
}
