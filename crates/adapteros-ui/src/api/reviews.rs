//! Reviews API client
//!
//! Client functions for the human-in-the-loop review queue.

use super::{ApiClient, ApiResult};
use adapteros_api_types::review::{ListPausedResponse, SubmitReviewRequest, SubmitReviewResponse};

impl ApiClient {
    /// Fetch all paused reviews from the queue
    pub async fn list_paused_reviews(&self) -> ApiResult<ListPausedResponse> {
        self.get("/v1/reviews/paused").await
    }

    /// Submit a review for a paused inference
    pub async fn submit_review(
        &self,
        request: &SubmitReviewRequest,
    ) -> ApiResult<SubmitReviewResponse> {
        self.post("/v1/reviews/submit", request).await
    }
}
