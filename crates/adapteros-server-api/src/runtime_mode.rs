//! Runtime mode configuration and enforcement
//!
//! Implements three runtime modes with different security and policy requirements:
//! - **Dev**: HTTP + HTTPS allowed, egress allowed, telemetry optional
//! - **Staging**: HTTP + UDS, egress allowlist only, telemetry required
//! - **Prod**: UDS only, HTTP/HTTPS disabled, egress deny-all, event signing required
//!
//! ## Resolution Order
//!
//! Mode is resolved with this precedence:
//! 1. Environment variable (`AOS_RUNTIME_MODE`)
//! 2. Database setting (`settings.runtime_mode`)
//! 3. Config file (`server.production_mode`)
//! 4. Default (dev)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_server::runtime_mode::{RuntimeMode, RuntimeModeResolver};
//! use adapteros_server::config::Config;
//! use adapteros_db::Db;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = Config::load("configs/cp.toml")?;
//! let db = Db::connect("var/aos-cp.sqlite3").await?;
//!
//! let mode = RuntimeModeResolver::resolve(&config, &db).await?;
//!
//! match mode {
//!     RuntimeMode::Prod => {
//!         // Enforce UDS-only, deny all egress
//!     }
//!     RuntimeMode::Staging => {
//!         // Check egress allowlist
//!     }
//!     RuntimeMode::Dev => {
//!         // Allow all operations
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! 【2025-11-25†feat(runtime)†mode-resolution】

use crate::config::Config;
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{info, warn};

/// Runtime mode determines security and policy enforcement level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    /// Development mode: relaxed security, all features enabled
    Dev,
    /// Staging mode: enhanced security, egress allowlist
    Staging,
    /// Production mode: maximum security, UDS-only, egress denied
    Prod,
}

impl RuntimeMode {
    /// Returns true if this is production mode
    pub fn is_prod(&self) -> bool {
        matches!(self, RuntimeMode::Prod)
    }

    /// Returns true if this is development mode
    pub fn is_dev(&self) -> bool {
        matches!(self, RuntimeMode::Dev)
    }

    /// Returns true if this is staging mode
    pub fn is_staging(&self) -> bool {
        matches!(self, RuntimeMode::Staging)
    }

    /// Get mode as string
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeMode::Dev => "dev",
            RuntimeMode::Staging => "staging",
            RuntimeMode::Prod => "prod",
        }
    }

    /// Returns true if HTTP binding is allowed in this mode
    pub fn allows_http(&self) -> bool {
        !self.is_prod()
    }

    /// Returns true if telemetry is required in this mode
    pub fn requires_telemetry(&self) -> bool {
        !self.is_dev()
    }

    /// Returns true if event signing is required in this mode
    pub fn requires_event_signing(&self) -> bool {
        self.is_prod()
    }

    /// Returns true if egress is allowed (any destination) in this mode
    pub fn allows_egress(&self) -> bool {
        self.is_dev()
    }

    /// Returns true if egress must be checked against allowlist
    pub fn requires_egress_allowlist(&self) -> bool {
        self.is_staging()
    }

    /// Returns true if egress is completely denied
    pub fn denies_egress(&self) -> bool {
        self.is_prod()
    }
}

impl std::fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for RuntimeMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dev" | "development" => Ok(RuntimeMode::Dev),
            "staging" | "stage" => Ok(RuntimeMode::Staging),
            "prod" | "production" => Ok(RuntimeMode::Prod),
            _ => Err(format!(
                "Invalid runtime mode: '{}'. Valid values: dev, staging, prod",
                s
            )),
        }
    }
}

/// Runtime mode resolver with precedence-based resolution
pub struct RuntimeModeResolver;

impl RuntimeModeResolver {
    /// Resolve runtime mode with precedence: env > db > config > default
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Environment variable is set but invalid
    /// - Database query fails
    pub async fn resolve(config: &Config, db: &Db) -> Result<RuntimeMode, String> {
        // 1. Check environment variable (highest precedence)
        if let Ok(mode_str) = std::env::var("AOS_RUNTIME_MODE") {
            info!(mode = %mode_str, "Runtime mode from environment variable");
            return mode_str
                .parse()
                .map_err(|e| format!("Failed to parse AOS_RUNTIME_MODE: {}", e));
        }

        // 2. Check database setting
        match Self::get_mode_from_db(db).await {
            Ok(Some(mode)) => {
                info!(mode = %mode, "Runtime mode from database settings");
                return Ok(mode);
            }
            Ok(None) => {
                // No database setting, continue to config
            }
            Err(e) => {
                warn!(error = %e, "Failed to query runtime mode from database, falling back to config");
            }
        }

        // 3. Check config file (backward compatibility via production_mode flag)
        if config.server.production_mode {
            info!("Runtime mode from config: prod (via production_mode flag)");
            return Ok(RuntimeMode::Prod);
        }

        // 4. Default to dev
        info!("Runtime mode defaulting to: dev");
        Ok(RuntimeMode::Dev)
    }

    /// Get runtime mode from database settings table
    async fn get_mode_from_db(db: &Db) -> Result<Option<RuntimeMode>, String> {
        let query = "SELECT value FROM settings WHERE key = 'runtime_mode' LIMIT 1";

        match sqlx::query_scalar::<_, String>(query)
            .fetch_optional(db.pool())
            .await
        {
            Ok(Some(value)) => value.parse().map(Some),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Database query failed: {}", e)),
        }
    }

    /// Validate runtime mode configuration
    ///
    /// Checks that the mode is compatible with other configuration settings.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Prod mode without UDS socket configured
    /// - Prod mode without JWT EdDSA mode
    /// - Prod mode without PF deny enforcement
    pub async fn validate(mode: RuntimeMode, config: &Config, db: &Db) -> Result<(), String> {
        if mode.is_prod() {
            // Production mode requires UDS socket
            if config.server.uds_socket.is_none() {
                return Err(
                    "Production mode requires UDS socket (server.uds_socket in config)".to_string(),
                );
            }

            // Production mode requires EdDSA JWT mode
            match Self::get_jwt_mode_from_db(db).await {
                Ok(Some(jwt_mode)) if jwt_mode.to_lowercase() != "eddsa" => {
                    return Err(format!(
                        "Production mode requires EdDSA JWT mode (current: {}). \
                         Set AOS_SECURITY_JWT_MODE=eddsa or update settings.jwt_mode in database.",
                        jwt_mode
                    ));
                }
                Ok(None) => {
                    return Err(
                        "Production mode requires EdDSA JWT mode but jwt_mode is not configured. \
                         Set AOS_SECURITY_JWT_MODE=eddsa or update settings.jwt_mode in database."
                            .to_string(),
                    );
                }
                Err(e) => {
                    return Err(format!("Failed to validate JWT mode for production: {}", e));
                }
                Ok(Some(_)) => {
                    // jwt_mode is "eddsa", validation passed
                }
            }

            // Production mode requires PF deny enforcement
            if !config.security.require_pf_deny {
                return Err(
                    "Production mode requires PF egress deny (security.require_pf_deny=true)"
                        .to_string(),
                );
            }
        }

        Ok(())
    }

    /// Get JWT mode from database settings table
    async fn get_jwt_mode_from_db(db: &Db) -> Result<Option<String>, String> {
        let query = "SELECT value FROM settings WHERE key = 'jwt_mode' LIMIT 1";

        match sqlx::query_scalar::<_, String>(query)
            .fetch_optional(db.pool())
            .await
        {
            Ok(value) => Ok(value),
            Err(e) => Err(format!("Database query failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_parsing() {
        assert_eq!("dev".parse::<RuntimeMode>().unwrap(), RuntimeMode::Dev);
        assert_eq!(
            "development".parse::<RuntimeMode>().unwrap(),
            RuntimeMode::Dev
        );
        assert_eq!(
            "staging".parse::<RuntimeMode>().unwrap(),
            RuntimeMode::Staging
        );
        assert_eq!(
            "stage".parse::<RuntimeMode>().unwrap(),
            RuntimeMode::Staging
        );
        assert_eq!("prod".parse::<RuntimeMode>().unwrap(), RuntimeMode::Prod);
        assert_eq!(
            "production".parse::<RuntimeMode>().unwrap(),
            RuntimeMode::Prod
        );

        assert!("invalid".parse::<RuntimeMode>().is_err());
    }

    #[test]
    fn test_mode_properties() {
        // Dev mode
        assert!(RuntimeMode::Dev.is_dev());
        assert!(RuntimeMode::Dev.allows_http());
        assert!(RuntimeMode::Dev.allows_egress());
        assert!(!RuntimeMode::Dev.requires_telemetry());
        assert!(!RuntimeMode::Dev.requires_event_signing());

        // Staging mode
        assert!(RuntimeMode::Staging.is_staging());
        assert!(RuntimeMode::Staging.allows_http());
        assert!(!RuntimeMode::Staging.allows_egress());
        assert!(RuntimeMode::Staging.requires_egress_allowlist());
        assert!(RuntimeMode::Staging.requires_telemetry());
        assert!(!RuntimeMode::Staging.requires_event_signing());

        // Prod mode
        assert!(RuntimeMode::Prod.is_prod());
        assert!(!RuntimeMode::Prod.allows_http());
        assert!(RuntimeMode::Prod.denies_egress());
        assert!(RuntimeMode::Prod.requires_telemetry());
        assert!(RuntimeMode::Prod.requires_event_signing());
    }

    #[test]
    fn test_mode_display() {
        assert_eq!(RuntimeMode::Dev.to_string(), "dev");
        assert_eq!(RuntimeMode::Staging.to_string(), "staging");
        assert_eq!(RuntimeMode::Prod.to_string(), "prod");
    }
}
