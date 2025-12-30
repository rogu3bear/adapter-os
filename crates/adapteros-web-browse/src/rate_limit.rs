//! Rate limiting for web browse requests
//!
//! Implements per-tenant rate limiting with minute and daily quotas.

use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    error::{WebBrowseError, WebBrowseResult},
    TenantId,
};

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per minute
    pub requests_per_minute: u32,

    /// Requests per day
    pub requests_per_day: u32,

    /// Enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 10,
            requests_per_day: 100,
            enabled: true,
        }
    }
}

/// Per-tenant rate limiter state
struct TenantRateLimiter {
    /// Minute-based rate limiter
    minute_limiter: GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>,

    /// Daily request counter
    daily_count: u32,

    /// Daily limit
    daily_limit: u32,

    /// Current day (for resetting daily counter)
    current_day: u32,
}

impl TenantRateLimiter {
    fn new(config: &RateLimitConfig) -> Self {
        let rpm = NonZeroU32::new(config.requests_per_minute.max(1)).unwrap();
        let quota = Quota::per_minute(rpm);

        Self {
            minute_limiter: GovernorRateLimiter::direct(quota),
            daily_count: 0,
            daily_limit: config.requests_per_day,
            current_day: current_day_number(),
        }
    }

    fn check_and_update(&mut self) -> Result<(), RateLimitError> {
        // Reset daily counter if day changed
        let today = current_day_number();
        if today != self.current_day {
            self.current_day = today;
            self.daily_count = 0;
        }

        // Check daily limit
        if self.daily_count >= self.daily_limit {
            return Err(RateLimitError::DailyLimitExceeded {
                count: self.daily_count,
                limit: self.daily_limit,
            });
        }

        // Check minute limit
        if self.minute_limiter.check().is_err() {
            return Err(RateLimitError::MinuteLimitExceeded);
        }

        // Increment daily counter
        self.daily_count += 1;

        Ok(())
    }

    fn status(&self) -> RateLimitStatus {
        let remaining_daily = self.daily_limit.saturating_sub(self.daily_count);

        RateLimitStatus {
            remaining_per_minute: 0, // Governor doesn't expose this easily
            remaining_per_day: remaining_daily,
            daily_count: self.daily_count,
            daily_limit: self.daily_limit,
        }
    }
}

/// Rate limit error types
#[derive(Debug)]
enum RateLimitError {
    MinuteLimitExceeded,
    #[allow(dead_code)]
    DailyLimitExceeded {
        count: u32,
        limit: u32,
    },
}

/// Rate limit status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    /// Remaining requests this minute (approximate)
    pub remaining_per_minute: u32,

    /// Remaining requests today
    pub remaining_per_day: u32,

    /// Requests made today
    pub daily_count: u32,

    /// Daily limit
    pub daily_limit: u32,
}

/// Rate limiter for web browse requests
pub struct RateLimiter {
    /// Per-tenant limiters
    limiters: Arc<RwLock<HashMap<TenantId, TenantRateLimiter>>>,

    /// Default config for new tenants
    default_config: RateLimitConfig,

    /// Whether rate limiting is enabled
    enabled: bool,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(default_config: RateLimitConfig) -> Self {
        let enabled = default_config.enabled;
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
            default_config,
            enabled,
        }
    }

    /// Check if request is allowed for tenant
    pub async fn check(&self, tenant_id: &TenantId) -> WebBrowseResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let mut limiters = self.limiters.write().await;

        let limiter = limiters
            .entry(tenant_id.clone())
            .or_insert_with(|| TenantRateLimiter::new(&self.default_config));

        match limiter.check_and_update() {
            Ok(()) => Ok(()),
            Err(RateLimitError::MinuteLimitExceeded) => Err(WebBrowseError::RateLimitExceeded {
                tenant_id: tenant_id.clone(),
                limit: self.default_config.requests_per_minute,
            }),
            Err(RateLimitError::DailyLimitExceeded { limit, .. }) => {
                Err(WebBrowseError::DailyQuotaExceeded {
                    tenant_id: tenant_id.clone(),
                    limit,
                })
            }
        }
    }

    /// Check with custom config (for tenant-specific limits)
    pub async fn check_with_config(
        &self,
        tenant_id: &TenantId,
        config: &RateLimitConfig,
    ) -> WebBrowseResult<()> {
        if !self.enabled || !config.enabled {
            return Ok(());
        }

        let mut limiters = self.limiters.write().await;

        // Get or create limiter with tenant-specific config
        let limiter = limiters
            .entry(tenant_id.clone())
            .or_insert_with(|| TenantRateLimiter::new(config));

        // Update limits if they changed
        if limiter.daily_limit != config.requests_per_day {
            limiter.daily_limit = config.requests_per_day;
        }

        match limiter.check_and_update() {
            Ok(()) => Ok(()),
            Err(RateLimitError::MinuteLimitExceeded) => Err(WebBrowseError::RateLimitExceeded {
                tenant_id: tenant_id.clone(),
                limit: config.requests_per_minute,
            }),
            Err(RateLimitError::DailyLimitExceeded { limit, .. }) => {
                Err(WebBrowseError::DailyQuotaExceeded {
                    tenant_id: tenant_id.clone(),
                    limit,
                })
            }
        }
    }

    /// Get rate limit status for tenant
    pub async fn status(&self, tenant_id: &TenantId) -> RateLimitStatus {
        let limiters = self.limiters.read().await;

        limiters
            .get(tenant_id)
            .map(|l| l.status())
            .unwrap_or(RateLimitStatus {
                remaining_per_minute: self.default_config.requests_per_minute,
                remaining_per_day: self.default_config.requests_per_day,
                daily_count: 0,
                daily_limit: self.default_config.requests_per_day,
            })
    }

    /// Reset rate limits for tenant (admin operation)
    pub async fn reset(&self, tenant_id: &TenantId) {
        let mut limiters = self.limiters.write().await;
        limiters.remove(tenant_id);
    }

    /// Get all tenant statuses (admin operation)
    pub async fn all_statuses(&self) -> HashMap<TenantId, RateLimitStatus> {
        let limiters = self.limiters.read().await;
        limiters
            .iter()
            .map(|(id, limiter)| (id.clone(), limiter.status()))
            .collect()
    }
}

/// Get current day number (days since epoch)
fn current_day_number() -> u32 {
    (chrono::Utc::now().timestamp() / 86400) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_requests() {
        let config = RateLimitConfig {
            requests_per_minute: 10,
            requests_per_day: 100,
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        let result = limiter.check(&"tenant1".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_daily_limit() {
        let config = RateLimitConfig {
            requests_per_minute: 100, // High minute limit
            requests_per_day: 3,      // Low daily limit
            enabled: true,
        };
        let limiter = RateLimiter::new(config);
        let tenant = "tenant1".to_string();

        // First 3 requests should succeed
        for _ in 0..3 {
            assert!(limiter.check(&tenant).await.is_ok());
        }

        // 4th request should fail
        let result = limiter.check(&tenant).await;
        assert!(matches!(
            result,
            Err(WebBrowseError::DailyQuotaExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            requests_per_minute: 1,
            requests_per_day: 1,
            enabled: false, // Disabled
        };
        let limiter = RateLimiter::new(config);

        // Should allow unlimited requests when disabled
        for _ in 0..10 {
            assert!(limiter.check(&"tenant1".to_string()).await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_status() {
        let config = RateLimitConfig {
            requests_per_minute: 10,
            requests_per_day: 100,
            enabled: true,
        };
        let limiter = RateLimiter::new(config);
        let tenant = "tenant1".to_string();

        // Make some requests
        limiter.check(&tenant).await.unwrap();
        limiter.check(&tenant).await.unwrap();

        let status = limiter.status(&tenant).await;
        assert_eq!(status.daily_count, 2);
        assert_eq!(status.remaining_per_day, 98);
    }
}
