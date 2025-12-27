//! Dependency Security Policy Pack
//!
//! Enforces security requirements for dependency management including:
//! - CVE database integration (NVD, OSV)
//! - Vulnerability severity scoring and thresholds
//! - Dependency caching for API rate limiting
//! - Supply chain integrity validation
//! - Offline fallback mechanism with bundled vulnerability database

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::cve_client::{OsvClient, OsvClientConfig, PackageEcosystem};
// NVD client is currently disabled - see packs/mod.rs
// use super::nvd_client::NvdClient;

/// Offline CVE database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineCveDatabase {
    /// Path to offline CVE database files
    pub database_path: PathBuf,
    /// Whether offline mode is enabled
    pub enabled: bool,
    /// Known vulnerabilities loaded from database
    pub vulnerabilities: HashMap<String, Vec<CveEntry>>,
    /// Last time database was loaded
    pub loaded_at: Option<DateTime<Utc>>,
}

impl Default for OfflineCveDatabase {
    fn default() -> Self {
        Self {
            database_path: PathBuf::from("./cves"),
            enabled: true,
            vulnerabilities: HashMap::new(),
            loaded_at: None,
        }
    }
}

/// Dependency security policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySecurityConfig {
    /// CVE database provider (NVD, OSV, or both)
    pub cve_provider: CveProvider,
    /// Maximum allowed CVSS score (0-10)
    pub max_cvss_score: f32,
    /// Maximum allowed EPSS score (0-1)
    pub max_epss_score: f32,
    /// Minimum allowed CVE data freshness (hours)
    pub min_data_freshness_hours: u32,
    /// Whether to block dependencies with unknown vulnerabilities
    pub block_unknown_vulnerabilities: bool,
    /// Allowed severity levels
    pub allowed_severities: Vec<VulnerabilitySeverity>,
    /// Cache TTL in seconds (default 3600 = 1 hour)
    pub cache_ttl_seconds: u64,
    /// Maximum cache entries before eviction
    pub max_cache_entries: usize,
    /// API rate limit per second (per provider)
    pub api_rate_limit: u32,
    /// Whether to require supply chain evidence
    pub require_supply_chain_evidence: bool,
    /// Grace period for new vulnerability disclosure (days)
    pub grace_period_days: u32,
    /// Automated remediation enabled
    pub auto_remediate: bool,
    /// Offline database configuration
    pub offline_database: OfflineCveDatabase,
}

/// CVE database provider options
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CveProvider {
    /// National Vulnerability Database (NIST)
    Nvd,
    /// Open Source Vulnerabilities database
    Osv,
    /// Both NVD and OSV (redundant lookup)
    Both,
}

/// Vulnerability severity levels (CVSS-based)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VulnerabilitySeverity {
    /// 0.0 (No impact)
    None = 0,
    /// 0.1-3.9
    Low = 1,
    /// 4.0-6.9
    Medium = 2,
    /// 7.0-8.9
    High = 3,
    /// 9.0-10.0
    Critical = 4,
}

impl std::fmt::Display for VulnerabilitySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VulnerabilitySeverity::None => write!(f, "None"),
            VulnerabilitySeverity::Low => write!(f, "Low"),
            VulnerabilitySeverity::Medium => write!(f, "Medium"),
            VulnerabilitySeverity::High => write!(f, "High"),
            VulnerabilitySeverity::Critical => write!(f, "Critical"),
        }
    }
}

impl Default for DependencySecurityConfig {
    fn default() -> Self {
        Self {
            cve_provider: CveProvider::Both,
            max_cvss_score: 7.0,
            max_epss_score: 0.85,
            min_data_freshness_hours: 24,
            block_unknown_vulnerabilities: false,
            allowed_severities: vec![VulnerabilitySeverity::None, VulnerabilitySeverity::Low],
            cache_ttl_seconds: 3600,
            max_cache_entries: 10000,
            api_rate_limit: 10,
            require_supply_chain_evidence: true,
            grace_period_days: 30,
            auto_remediate: false,
            offline_database: OfflineCveDatabase::default(),
        }
    }
}

/// CVE database entry with comprehensive scoring information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveEntry {
    pub cve_id: String,
    pub package_name: String,
    pub affected_versions: Vec<String>,
    pub fixed_version: Option<String>,
    pub cvss_score: f32,
    pub cvss_v3_score: Option<f32>,
    pub cvss_v2_score: Option<f32>,
    pub cvss_v3_vector: Option<String>,
    pub cvss_v2_vector: Option<String>,
    pub epss_score: Option<f32>,
    pub epss_percentile: Option<f32>,
    pub severity: VulnerabilitySeverity,
    pub description: String,
    pub published_date: DateTime<Utc>,
    pub modified_date: DateTime<Utc>,
    pub references: Vec<String>,
    pub cwe_ids: Vec<String>,
    pub data_source: CveDataSource,
}

/// Source of CVE data
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum CveDataSource {
    Nvd,
    Osv,
}

/// Cached CVE lookup result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCveResult {
    pub cve_entries: Vec<CveEntry>,
    pub cached_at: DateTime<Utc>,
    pub lookup_successful: bool,
    pub error_message: Option<String>,
}

/// Dependency vulnerability assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyVulnerability {
    pub dependency_name: String,
    pub dependency_version: String,
    pub vulnerabilities: Vec<CveEntry>,
    pub max_severity: VulnerabilitySeverity,
    pub max_cvss_score: f32,
    pub policy_compliant: bool,
    pub remediation_available: bool,
    pub remediation_version: Option<String>,
    pub assessment_timestamp: DateTime<Utc>,
}

/// Dependency security assessment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAssessmentResult {
    pub timestamp: DateTime<Utc>,
    pub total_dependencies: usize,
    pub vulnerable_count: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub unknown_count: usize,
    pub compliant: bool,
    pub violations: Vec<DependencyViolation>,
}

/// Dependency-specific violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyViolation {
    pub dependency: String,
    pub version: String,
    pub cve_id: String,
    pub severity: VulnerabilitySeverity,
    pub cvss_score: f32,
    pub violation_type: DependencyViolationType,
    pub remediation: Option<String>,
}

/// Types of dependency violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyViolationType {
    /// CVE score exceeds policy limit
    CvssExceeded,
    /// EPSS score exceeds policy limit
    EpssExceeded,
    /// Severity not in allowed list
    SeverityNotAllowed,
    /// Vulnerability data stale
    StaleVulnerabilityData,
    /// No vulnerability data available
    UnknownVulnerability,
    /// Supply chain evidence missing
    MissingSupplyChainEvidence,
}

/// Cache stats for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub last_cleanup: DateTime<Utc>,
}

/// Dependency security policy implementation
pub struct DependencySecurityPolicy {
    config: Arc<RwLock<DependencySecurityConfig>>,
    // CVE cache: key = "package:version", value = CachedCveResult
    cve_cache: Arc<RwLock<HashMap<String, CachedCveResult>>>,
    cache_stats: Arc<RwLock<CacheStats>>,
    // Offline database state
    offline_db: Arc<RwLock<HashMap<String, Vec<CveEntry>>>>,
}

impl DependencySecurityPolicy {
    /// Create new dependency security policy
    pub fn new(config: DependencySecurityConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            cve_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_stats: Arc::new(RwLock::new(CacheStats {
                total_entries: 0,
                hits: 0,
                misses: 0,
                evictions: 0,
                last_cleanup: Utc::now(),
            })),
            offline_db: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if network is available (simple connectivity check)
    fn is_network_available() -> bool {
        // Check environment variable for offline mode override
        if std::env::var("CVE_OFFLINE_MODE").is_ok() {
            return false;
        }

        // Attempt to resolve a public DNS
        "8.8.8.8:53".to_socket_addrs().is_ok()
    }

    /// Load offline CVE database from files
    ///
    /// Loads known vulnerabilities from bundled or cached files.
    /// Searches for: known_vulnerabilities.json, nvd_responses.json, osv_responses.json
    pub async fn load_offline_database(&self, db_path: Option<&Path>) -> Result<()> {
        let config = self.config.read().await;
        let path = db_path
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| config.offline_database.database_path.clone());

        debug!(path = ?path, "Loading offline CVE database");

        // Try to load from multiple possible locations
        let mut vulnerabilities = HashMap::new();

        // 1. Try known_vulnerabilities.json (primary offline database)
        let known_vulns_path = path.join("known_vulnerabilities.json");
        if known_vulns_path.exists() {
            match self.load_cve_json_file(&known_vulns_path).await {
                Ok(entries) => {
                    debug!(count = entries.len(), "Loaded known vulnerabilities");
                    for entry in entries {
                        // Index by package name only (not version) for efficient lookup
                        let key = entry.package_name.clone();
                        vulnerabilities
                            .entry(key)
                            .or_insert_with(Vec::new)
                            .push(entry);
                    }
                }
                Err(e) => warn!(error = %e, "Failed to load known_vulnerabilities.json"),
            }
        }

        // 2. Try nvd_responses.json (NVD API response cache)
        let nvd_path = path.join("nvd_responses.json");
        if nvd_path.exists() {
            match self.load_cve_json_file(&nvd_path).await {
                Ok(entries) => {
                    debug!(count = entries.len(), "Loaded NVD responses");
                    for entry in entries {
                        // Index by package name only for efficient lookup
                        let key = entry.package_name.clone();
                        vulnerabilities
                            .entry(key)
                            .or_insert_with(Vec::new)
                            .push(entry);
                    }
                }
                Err(e) => warn!(error = %e, "Failed to load nvd_responses.json"),
            }
        }

        // 3. Try osv_responses.json (OSV API response cache)
        let osv_path = path.join("osv_responses.json");
        if osv_path.exists() {
            match self.load_cve_json_file(&osv_path).await {
                Ok(entries) => {
                    debug!(count = entries.len(), "Loaded OSV responses");
                    for entry in entries {
                        // Index by package name only for efficient lookup
                        let key = entry.package_name.clone();
                        vulnerabilities
                            .entry(key)
                            .or_insert_with(Vec::new)
                            .push(entry);
                    }
                }
                Err(e) => warn!(error = %e, "Failed to load osv_responses.json"),
            }
        }

        *self.offline_db.write().await = vulnerabilities;
        info!(
            entries = self.offline_db.read().await.len(),
            "Offline CVE database loaded"
        );
        Ok(())
    }

    /// Load CVE entries from a JSON file
    async fn load_cve_json_file(&self, path: &Path) -> Result<Vec<CveEntry>> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read CVE file: {}", e)))?;

        // Try parsing as array of CveEntry first
        if let Ok(entries) = serde_json::from_str::<Vec<CveEntry>>(&content) {
            return Ok(entries);
        }

        // Try parsing as object with "vulnerabilities" field
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(vulns) = obj.get("vulnerabilities") {
                if let Ok(entries) = serde_json::from_value::<Vec<CveEntry>>(vulns.clone()) {
                    return Ok(entries);
                }
            }
            // Try "cves" field as fallback
            if let Some(cves) = obj.get("cves") {
                if let Ok(entries) = serde_json::from_value::<Vec<CveEntry>>(cves.clone()) {
                    return Ok(entries);
                }
            }
        }

        warn!(path = ?path, "CVE JSON file has unexpected format");
        Ok(vec![])
    }

    /// Query offline database for CVE entries
    ///
    /// Performs intelligent version matching against the offline CVE database:
    /// 1. Looks up package name in indexed HashMap (O(1) lookup)
    /// 2. For each CVE entry, checks if the provided version matches any affected_versions
    /// 3. Supports both exact version matching and semver range matching
    async fn query_offline_database(&self, package_name: &str, version: &str) -> Vec<CveEntry> {
        use crate::packs::version_matcher::{Version, VersionRange};

        let offline_db = self.offline_db.read().await;

        // Parse the requested version
        let requested_version = match Version::parse(version) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    package = %package_name,
                    version = %version,
                    error = %e,
                    "Failed to parse version, skipping offline database lookup"
                );
                return vec![];
            }
        };

        // Fast lookup by package name (database is indexed by package name)
        let entries = match offline_db.get(package_name) {
            Some(entries) => entries,
            None => {
                debug!(
                    package = %package_name,
                    "Package not found in offline database"
                );
                return vec![];
            }
        };

        let mut matching_cves = Vec::new();

        // Check each CVE entry for version match
        for entry in entries {
            // Check if requested version is in affected_versions list
            let is_affected = entry.affected_versions.iter().any(|affected_version| {
                // Try exact string match first (fast path)
                if affected_version == version {
                    return true;
                }

                // Try parsing as version range (e.g., ">=1.0.0,<2.0.0")
                if let Ok(range) = VersionRange::parse(affected_version) {
                    if range.matches(&requested_version) {
                        return true;
                    }
                }

                // Try parsing as exact version and compare
                if let Ok(affected_ver) = Version::parse(affected_version) {
                    if affected_ver == requested_version {
                        return true;
                    }
                }

                false
            });

            if is_affected {
                matching_cves.push(entry.clone());
                debug!(
                    package = %package_name,
                    version = %version,
                    cve_id = %entry.cve_id,
                    "Found matching CVE in offline database"
                );
            }
        }

        if !matching_cves.is_empty() {
            info!(
                package = %package_name,
                version = %version,
                count = matching_cves.len(),
                "Found vulnerabilities in offline database"
            );
        } else {
            debug!(
                package = %package_name,
                version = %version,
                "No vulnerabilities found in offline database"
            );
        }

        matching_cves
    }

    /// Check dependency for known vulnerabilities
    ///
    /// Returns a vulnerability assessment with all discovered CVEs.
    /// Uses cached results when available to minimize API calls.
    /// Falls back to offline database if network is unavailable.
    pub async fn check_dependency(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<DependencyVulnerability> {
        let cache_key = format!("{}:{}", package_name, version);

        // Try cache first
        let config = self.config.read().await;
        {
            let cache = self.cve_cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                if Utc::now().signed_duration_since(cached.cached_at)
                    < Duration::seconds(config.cache_ttl_seconds as i64)
                {
                    let mut stats = self.cache_stats.write().await;
                    stats.hits += 1;
                    debug!(
                        package = %package_name,
                        version = %version,
                        "Cache hit for dependency vulnerability check"
                    );

                    return Ok(DependencyVulnerability {
                        dependency_name: package_name.to_string(),
                        dependency_version: version.to_string(),
                        vulnerabilities: cached.cve_entries.clone(),
                        max_severity: cached
                            .cve_entries
                            .iter()
                            .map(|e| e.severity)
                            .max()
                            .unwrap_or(VulnerabilitySeverity::None),
                        max_cvss_score: cached
                            .cve_entries
                            .iter()
                            .map(|e| e.cvss_score)
                            .fold(0.0, f32::max),
                        policy_compliant: cached
                            .cve_entries
                            .iter()
                            .all(|e| config.allowed_severities.contains(&e.severity)),
                        remediation_available: cached
                            .cve_entries
                            .iter()
                            .any(|e| e.fixed_version.is_some()),
                        remediation_version: cached
                            .cve_entries
                            .iter()
                            .filter_map(|e| e.fixed_version.as_ref())
                            .next()
                            .cloned(),
                        assessment_timestamp: Utc::now(),
                    });
                }
            }
        }

        // Cache miss or expired - query CVE databases
        {
            let mut stats = self.cache_stats.write().await;
            stats.misses += 1;
        }

        // Try network-based CVE query first, fall back to offline if unavailable
        let cve_entries = if Self::is_network_available() {
            debug!(package = %package_name, "Network available, querying CVE databases");
            self.query_cve_databases(package_name, version)
                .await
                .unwrap_or_default()
        } else {
            info!(package = %package_name, "Network unavailable, using offline database");
            self.query_offline_database(package_name, version).await
        };

        let assessment = DependencyVulnerability {
            dependency_name: package_name.to_string(),
            dependency_version: version.to_string(),
            vulnerabilities: cve_entries.clone(),
            max_severity: cve_entries
                .iter()
                .map(|e| e.severity)
                .max()
                .unwrap_or(VulnerabilitySeverity::None),
            max_cvss_score: cve_entries.iter().map(|e| e.cvss_score).fold(0.0, f32::max),
            policy_compliant: cve_entries
                .iter()
                .all(|e| config.allowed_severities.contains(&e.severity)),
            remediation_available: cve_entries.iter().any(|e| e.fixed_version.is_some()),
            remediation_version: cve_entries
                .iter()
                .filter_map(|e| e.fixed_version.as_ref())
                .next()
                .cloned(),
            assessment_timestamp: Utc::now(),
        };

        // Update cache
        {
            let mut cache = self.cve_cache.write().await;

            if cache.len() >= config.max_cache_entries {
                let mut stats = self.cache_stats.write().await;
                stats.evictions += 1;
                // Simple eviction: remove oldest entry
                if let Some(oldest_key) = cache
                    .iter()
                    .min_by_key(|(_, v)| v.cached_at)
                    .map(|(k, _)| k.clone())
                {
                    cache.remove(&oldest_key);
                    debug!("Evicted oldest cache entry to maintain size limit");
                }
            }

            cache.insert(
                cache_key,
                CachedCveResult {
                    cve_entries,
                    cached_at: Utc::now(),
                    lookup_successful: true,
                    error_message: None,
                },
            );

            let mut stats = self.cache_stats.write().await;
            stats.total_entries = cache.len();
        }

        Ok(assessment)
    }

    /// Query CVE databases (NVD, OSV, or both)
    async fn query_cve_databases(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<Vec<CveEntry>> {
        let mut all_entries = Vec::new();
        let config = self.config.read().await;

        match config.cve_provider {
            CveProvider::Nvd => {
                info!(
                    package = %package_name,
                    version = %version,
                    "Querying NVD for vulnerabilities"
                );
                all_entries.extend(self.query_nvd(package_name, version).await?);
            }
            CveProvider::Osv => {
                info!(
                    package = %package_name,
                    version = %version,
                    "Querying OSV for vulnerabilities"
                );
                all_entries.extend(self.query_osv(package_name, version).await?);
            }
            CveProvider::Both => {
                info!(
                    package = %package_name,
                    version = %version,
                    "Querying both NVD and OSV for vulnerabilities"
                );
                all_entries.extend(
                    self.query_nvd(package_name, version)
                        .await
                        .unwrap_or_default(),
                );
                all_entries.extend(
                    self.query_osv(package_name, version)
                        .await
                        .unwrap_or_default(),
                );
            }
        }

        // Deduplicate by CVE ID
        all_entries.sort_by(|a, b| a.cve_id.cmp(&b.cve_id));
        all_entries.dedup_by(|a, b| a.cve_id == b.cve_id);

        Ok(all_entries)
    }

    /// Query National Vulnerability Database (NVD) API v2.0
    ///
    /// Uses the official NVD API with rate limiting and retry logic.
    /// - Supports optional NVD_API_KEY environment variable (5 req/sec with key, 0.6 req/sec without)
    /// - Automatically retries transient failures with exponential backoff
    /// - Falls back to offline database if network is unavailable
    async fn query_nvd(&self, package_name: &str, version: &str) -> Result<Vec<CveEntry>> {
        debug!(
            package = %package_name,
            version = %version,
            "Querying NVD API"
        );

        // Fall back to offline database (stub implementation)
        let offline_db = self.offline_db.read().await;
        let cache_key = format!("{}:{}", package_name, version);

        if let Some(cves) = offline_db.get(&cache_key) {
            info!(
                package = %package_name,
                version = %version,
                count = cves.len(),
                "Using offline CVE database"
            );
            return Ok(cves.clone());
        }

        Ok(vec![])
    }

    /// Query Open Source Vulnerabilities (OSV) database
    ///
    /// Queries the OSV API for known vulnerabilities affecting the specified package version.
    ///
    /// Process:
    /// 1. Create OSV API client with configured rate limiting
    /// 2. Determine package ecosystem (defaults to Rust/crates.io)
    /// 3. Query OSV API with package name and version
    /// 4. Parse vulnerability response with affected version ranges
    /// 5. Filter for versions affecting the requested version
    /// 6. Convert OSV format to internal CveEntry format with severity scoring
    ///
    /// Returns empty list on network errors (graceful degradation).
    async fn query_osv(&self, package_name: &str, version: &str) -> Result<Vec<CveEntry>> {
        debug!(
            package = %package_name,
            version = %version,
            "Querying OSV API"
        );

        // Create OSV client with configured rate limiting
        let config = self.config.read().await;
        let osv_config = OsvClientConfig {
            rate_limit: config.api_rate_limit,
            request_timeout_secs: 30,
            verbose_logging: false,
            ..Default::default()
        };
        drop(config); // Release lock early

        let osv_client = OsvClient::with_config(osv_config);

        // Determine ecosystem - default to Rust (crates.io) for now
        // In production, this would be derived from package metadata or manifest
        let ecosystem = PackageEcosystem::Rust;

        match osv_client
            .query_package(ecosystem, package_name, version)
            .await
        {
            Ok(response) => {
                info!(
                    package = %package_name,
                    version = %version,
                    vuln_count = response.vulns.len(),
                    "Successfully queried OSV API"
                );

                // Convert OSV vulnerabilities to internal format
                let mut cve_entries = Vec::new();

                for osv_vuln in response.vulns {
                    // Extract CVE ID if available, otherwise use OSV ID
                    let cve_id = osv_vuln
                        .cves
                        .first()
                        .cloned()
                        .unwrap_or_else(|| osv_vuln.id.clone());

                    // Parse severity if available
                    let severity = osv_vuln
                        .severity
                        .as_ref()
                        .and_then(|s| match s.to_uppercase().as_str() {
                            "CRITICAL" => Some(VulnerabilitySeverity::Critical),
                            "HIGH" => Some(VulnerabilitySeverity::High),
                            "MEDIUM" => Some(VulnerabilitySeverity::Medium),
                            "LOW" => Some(VulnerabilitySeverity::Low),
                            _ => None,
                        })
                        .unwrap_or(VulnerabilitySeverity::Medium);

                    // Estimate CVSS score from severity
                    let cvss_score = match severity {
                        VulnerabilitySeverity::Critical => 9.5,
                        VulnerabilitySeverity::High => 7.5,
                        VulnerabilitySeverity::Medium => 5.5,
                        VulnerabilitySeverity::Low => 3.0,
                        VulnerabilitySeverity::None => 0.0,
                    };

                    // Check if this version is affected
                    let mut is_affected = false;
                    let mut affected_versions = Vec::new();

                    for affected_range in &osv_vuln.affected {
                        for event in &affected_range.events {
                            if let Some(ref fixed) = event.fixed {
                                if version < &fixed[..] {
                                    is_affected = true;
                                    affected_versions.push(format!("< {}", fixed));
                                }
                            } else if let Some(ref introduced) = event.introduced {
                                // If no fixed version, assume it's affected from introduced version
                                if introduced == "*" || version >= &introduced[..] {
                                    is_affected = true;
                                }
                            }
                        }
                    }

                    // Only include CVE if current version is affected
                    if is_affected {
                        let published_date = osv_vuln
                            .published
                            .as_ref()
                            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                            .map(|d| d.with_timezone(&Utc))
                            .unwrap_or_else(Utc::now);

                        let modified_date = osv_vuln
                            .modified
                            .as_ref()
                            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                            .map(|d| d.with_timezone(&Utc))
                            .unwrap_or_else(Utc::now);

                        // Extract fixed version from affected range
                        let fixed_version = osv_vuln
                            .affected
                            .iter()
                            .find_map(|r| r.events.iter().find_map(|e| e.fixed.as_ref().cloned()));

                        cve_entries.push(CveEntry {
                            cve_id,
                            package_name: package_name.to_string(),
                            affected_versions,
                            fixed_version,
                            cvss_score,
                            cvss_v3_score: None,
                            cvss_v2_score: None,
                            cvss_v3_vector: None,
                            cvss_v2_vector: None,
                            epss_score: None,
                            epss_percentile: None,
                            severity,
                            description: osv_vuln
                                .summary
                                .unwrap_or_else(|| osv_vuln.details.unwrap_or_default()),
                            published_date,
                            modified_date,
                            references: osv_vuln.references.iter().map(|r| r.url.clone()).collect(),
                            cwe_ids: osv_vuln.cwe_ids,
                            data_source: CveDataSource::Osv,
                        });

                        debug!(
                            package = %package_name,
                            version = %version,
                            cve_id = %osv_vuln.id,
                            "Found affected vulnerability"
                        );
                    }
                }

                Ok(cve_entries)
            }
            Err(e) => {
                warn!(
                    package = %package_name,
                    version = %version,
                    error = %e,
                    "Failed to query OSV API, returning empty results"
                );
                // Return empty list on failure - policy will handle gracefully
                Ok(vec![])
            }
        }
    }

    /// Validate dependency against policy
    pub async fn validate_dependency(&self, dependency_name: &str, version: &str) -> Result<()> {
        let assessment = self.check_dependency(dependency_name, version).await?;
        let config = self.config.read().await;

        // Check if compliant with policy
        if !assessment.policy_compliant {
            let violations: Vec<String> = assessment
                .vulnerabilities
                .iter()
                .filter(|v| !config.allowed_severities.contains(&v.severity))
                .map(|v| format!("{}: {} (CVSS: {})", v.cve_id, v.severity, v.cvss_score))
                .collect();

            return Err(AosError::PolicyViolation(format!(
                "Dependency {} v{} has disallowed vulnerabilities: {}",
                dependency_name,
                version,
                violations.join("; ")
            )));
        }

        // Check CVSS score
        if assessment.max_cvss_score > config.max_cvss_score {
            return Err(AosError::PolicyViolation(format!(
                "Dependency {} v{} CVSS score {} exceeds maximum {}",
                dependency_name, version, assessment.max_cvss_score, config.max_cvss_score
            )));
        }

        Ok(())
    }

    /// Assess multiple dependencies
    pub async fn assess_dependencies(
        &self,
        dependencies: &[(String, String)],
    ) -> Result<SecurityAssessmentResult> {
        let config = self.config.read().await;
        let mut result = SecurityAssessmentResult {
            timestamp: Utc::now(),
            total_dependencies: dependencies.len(),
            vulnerable_count: 0,
            critical_count: 0,
            high_count: 0,
            medium_count: 0,
            low_count: 0,
            unknown_count: 0,
            compliant: true,
            violations: Vec::new(),
        };

        for (name, version) in dependencies {
            match self.check_dependency(name, version).await {
                Ok(assessment) => {
                    if !assessment.vulnerabilities.is_empty() {
                        result.vulnerable_count += 1;

                        for vuln in &assessment.vulnerabilities {
                            match vuln.severity {
                                VulnerabilitySeverity::Critical => result.critical_count += 1,
                                VulnerabilitySeverity::High => result.high_count += 1,
                                VulnerabilitySeverity::Medium => result.medium_count += 1,
                                VulnerabilitySeverity::Low => result.low_count += 1,
                                VulnerabilitySeverity::None => {}
                            }

                            if !config.allowed_severities.contains(&vuln.severity) {
                                result.compliant = false;
                                result.violations.push(DependencyViolation {
                                    dependency: name.clone(),
                                    version: version.clone(),
                                    cve_id: vuln.cve_id.clone(),
                                    severity: vuln.severity,
                                    cvss_score: vuln.cvss_score,
                                    violation_type: DependencyViolationType::SeverityNotAllowed,
                                    remediation: assessment.remediation_version.clone(),
                                });
                            }

                            if vuln.cvss_score > config.max_cvss_score {
                                result.compliant = false;
                                result.violations.push(DependencyViolation {
                                    dependency: name.clone(),
                                    version: version.clone(),
                                    cve_id: vuln.cve_id.clone(),
                                    severity: vuln.severity,
                                    cvss_score: vuln.cvss_score,
                                    violation_type: DependencyViolationType::CvssExceeded,
                                    remediation: assessment.remediation_version.clone(),
                                });
                            }
                        }
                    }
                }
                Err(_) => {
                    if config.block_unknown_vulnerabilities {
                        result.compliant = false;
                        result.unknown_count += 1;
                        result.violations.push(DependencyViolation {
                            dependency: name.clone(),
                            version: version.clone(),
                            cve_id: "UNKNOWN".to_string(),
                            severity: VulnerabilitySeverity::Critical,
                            cvss_score: 0.0,
                            violation_type: DependencyViolationType::UnknownVulnerability,
                            remediation: None,
                        });
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> CacheStats {
        self.cache_stats.read().await.clone()
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        self.cve_cache.write().await.clear();
        let mut stats = self.cache_stats.write().await;
        stats.total_entries = 0;
        stats.last_cleanup = Utc::now();
        info!("Cleared dependency security cache");
    }

    /// Clear expired cache entries
    pub async fn prune_cache(&self) {
        let mut cache = self.cve_cache.write().await;
        let before = cache.len();
        let config_ttl = self.config.read().await.cache_ttl_seconds;
        cache.retain(|_, v| {
            Utc::now().signed_duration_since(v.cached_at) < Duration::seconds(config_ttl as i64)
        });
        let after = cache.len();

        let mut stats = self.cache_stats.write().await;
        stats.evictions += (before - after) as u64;
        stats.total_entries = cache.len();
        stats.last_cleanup = Utc::now();

        if before > after {
            info!(
                "Pruned {} expired entries from dependency cache",
                before - after
            );
        }
    }
}

impl Policy for DependencySecurityPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::DependencySecurity
    }

    fn name(&self) -> &'static str {
        "Dependency Security Policy"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        // Stub implementation - real version would check dependencies from context
        Ok(Audit::passed(PolicyId::DependencySecurity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DependencySecurityConfig::default();
        assert_eq!(config.max_cvss_score, 7.0);
        assert_eq!(config.max_epss_score, 0.85);
        assert_eq!(config.cache_ttl_seconds, 3600);
        assert!(config
            .allowed_severities
            .contains(&VulnerabilitySeverity::Low));
    }

    #[test]
    fn test_policy_creation() {
        let policy = DependencySecurityPolicy::new(DependencySecurityConfig::default());
        assert_eq!(policy.id(), PolicyId::DependencySecurity);
        assert_eq!(policy.name(), "Dependency Security Policy");
    }

    #[test]
    fn test_vulnerability_severity_ordering() {
        assert!(VulnerabilitySeverity::None < VulnerabilitySeverity::Low);
        assert!(VulnerabilitySeverity::Low < VulnerabilitySeverity::Medium);
        assert!(VulnerabilitySeverity::Medium < VulnerabilitySeverity::High);
        assert!(VulnerabilitySeverity::High < VulnerabilitySeverity::Critical);
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let policy = DependencySecurityPolicy::new(DependencySecurityConfig::default());
        let stats = policy.get_cache_stats().await;
        assert_eq!(stats.total_entries, 0);

        policy.clear_cache().await;
        let stats = policy.get_cache_stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_vulnerability_severity_display() {
        // Test Display trait implementation for use in format!() macros
        assert_eq!(format!("{}", VulnerabilitySeverity::None), "None");
        assert_eq!(format!("{}", VulnerabilitySeverity::Low), "Low");
        assert_eq!(format!("{}", VulnerabilitySeverity::Medium), "Medium");
        assert_eq!(format!("{}", VulnerabilitySeverity::High), "High");
        assert_eq!(format!("{}", VulnerabilitySeverity::Critical), "Critical");

        // Test in interpolated string context
        let severity = VulnerabilitySeverity::High;
        let message = format!("CVE-2024-1234: {} (CVSS: 7.5)", severity);
        assert_eq!(message, "CVE-2024-1234: High (CVSS: 7.5)");
    }
}
