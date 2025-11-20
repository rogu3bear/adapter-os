//! Request timeout utilities
//!
//! Provides timeout functionality for inference requests to prevent indefinite hangs.

use crate::{AosError, Result};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{self, Sleep};

/// Timeout wrapper for futures
pub struct Timeout<F> {
    future: Pin<Box<F>>,
    sleep: Pin<Box<Sleep>>,
}

impl<F> Timeout<F> {
    /// Create a new timeout wrapper
    pub fn new(future: F, duration: Duration) -> Self {
        Self {
            future: Box::pin(future),
            sleep: Box::pin(time::sleep(duration)),
        }
    }
}

impl<F: Future> Future for Timeout<F> {
    type Output = Result<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll the sleep future first
        if Pin::new(&mut self.sleep).poll(cx).is_ready() {
            return Poll::Ready(Err(AosError::Timeout {
                duration: Duration::from_secs(30), // Default 30s
            }));
        }

        // Poll the actual future
        match Pin::new(&mut self.future).poll(cx) {
            Poll::Ready(result) => Poll::Ready(Ok(result)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Extension trait for adding timeout to futures
pub trait TimeoutExt: Future + Sized {
    /// Add a timeout to this future
    fn with_timeout(self, duration: Duration) -> Timeout<Self> {
        Timeout::new(self, duration)
    }
}

impl<F: Future> TimeoutExt for F {}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_timeout_success() {
        let future = async {
            sleep(Duration::from_millis(10)).await;
            "success"
        };

        let result = future.with_timeout(Duration::from_millis(100)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_timeout_failure() {
        let future = async {
            sleep(Duration::from_millis(100)).await;
            "too late"
        };

        let result = future.with_timeout(Duration::from_millis(50)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AosError::Timeout { duration } => {
                assert_eq!(duration, Duration::from_secs(30));
            }
            _ => panic!("Expected timeout error"),
        }
    }
}
