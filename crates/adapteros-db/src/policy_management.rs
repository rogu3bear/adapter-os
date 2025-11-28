//! Policy Management - Database methods for PRD-GOV-01
//!
//! Provides database operations for policy packs, policy assignments, violations,
//! and compliance scoring. Supports the 23 canonical policy packs with Ed25519 signing.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Policy pack record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PolicyPack {
    pub id: String,
    pub version: String,
    pub policy_type: String,
    pub content_json: String,
    pub signature: String,
    pub public_key: String,
    pub hash_b3: String,
    pub status: String,
    pub description: Option<String>,
    pub created_at: String,
    pub created_by: String,
    pub activated_at: Option<String>,
    pub deprecated_at: Option<String>,
    pub metadata_json: Option<String>,
}

/// Policy assignment record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PolicyAssignment {
    pub id: String,
    pub policy_pack_id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub priority: i32,
    pub enforced: bool,
    pub assigned_at: String,
    pub assigned_by: String,
    pub expires_at: Option<String>,
    pub metadata_json: Option<String>,
}

/// Policy violation record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PolicyViolation {
    pub id: String,
    pub policy_pack_id: String,
    pub policy_assignment_id: Option<String>,
    pub violation_type: String,
    pub severity: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub tenant_id: String,
    pub violation_message: String,
    pub violation_details_json: Option<String>,
    pub detected_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
    pub resolution_notes: Option<String>,
}

/// Compliance score record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ComplianceScore {
    pub id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub policy_pack_id: Option<String>,
    pub score: f64,
    pub total_checks: i32,
    pub passed_checks: i32,
    pub failed_checks: i32,
    pub critical_violations: i32,
    pub high_violations: i32,
    pub medium_violations: i32,
    pub low_violations: i32,
    pub calculated_at: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub metadata_json: Option<String>,
}

/// Category compliance score (for stack compliance summary)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryComplianceData {
    pub score: f64,
    pub passed: i32,
    pub failed: i32,
}

/// Stack compliance summary (calculated from violations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackComplianceData {
    pub overall_score: f64,
    pub status: String,
    pub by_category: std::collections::HashMap<String, CategoryComplianceData>,
    pub last_calculated: String,
}

impl Db {
    // ========== Policy Pack Methods ==========

    /// Store a signed policy pack
    pub async fn store_policy_pack(
        &self,
        id: &str,
        version: &str,
        policy_type: &str,
        content_json: &str,
        signature: &str,
        public_key: &str,
        hash_b3: &str,
        created_by: &str,
        description: Option<&str>,
    ) -> Result<String> {
        let created_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO policy_packs
             (id, version, policy_type, content_json, signature, public_key, hash_b3,
              status, description, created_at, created_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'draft', ?, ?, ?)",
        )
        .bind(id)
        .bind(version)
        .bind(policy_type)
        .bind(content_json)
        .bind(signature)
        .bind(public_key)
        .bind(hash_b3)
        .bind(description)
        .bind(&created_at)
        .bind(created_by)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to store policy pack: {}", e)))?;

        Ok(id.to_string())
    }

    /// Get policy pack by ID
    pub async fn get_policy_pack(&self, id: &str) -> Result<Option<PolicyPack>> {
        let pack = sqlx::query_as::<_, PolicyPack>("SELECT * FROM policy_packs WHERE id = ?")
            .bind(id)
            .fetch_optional(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch policy pack: {}", e)))?;

        Ok(pack)
    }

    /// List policy packs with optional filters
    pub async fn list_policy_packs(
        &self,
        policy_type: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<PolicyPack>> {
        let mut query = String::from("SELECT * FROM policy_packs WHERE 1=1");
        let mut params: Vec<String> = Vec::new();

        if let Some(pt) = policy_type {
            query.push_str(" AND policy_type = ?");
            params.push(pt.to_string());
        }

        if let Some(s) = status {
            query.push_str(" AND status = ?");
            params.push(s.to_string());
        }

        query.push_str(" ORDER BY created_at DESC");

        let mut q = sqlx::query_as::<_, PolicyPack>(&query);
        for param in &params {
            q = q.bind(param);
        }

        let packs = q
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list policy packs: {}", e)))?;

        Ok(packs)
    }

    /// Activate a policy pack
    pub async fn activate_policy_pack(&self, id: &str) -> Result<()> {
        let activated_at = chrono::Utc::now().to_rfc3339();

        sqlx::query("UPDATE policy_packs SET status = 'active', activated_at = ? WHERE id = ?")
            .bind(&activated_at)
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to activate policy pack: {}", e)))?;

        Ok(())
    }

    /// Deprecate a policy pack
    pub async fn deprecate_policy_pack(&self, id: &str) -> Result<()> {
        let deprecated_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE policy_packs SET status = 'deprecated', deprecated_at = ? WHERE id = ?",
        )
        .bind(&deprecated_at)
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to deprecate policy pack: {}", e)))?;

        Ok(())
    }

    // ========== Policy Assignment Methods ==========

    /// Assign a policy pack to a target
    pub async fn assign_policy(
        &self,
        policy_pack_id: &str,
        target_type: &str,
        target_id: Option<&str>,
        assigned_by: &str,
        priority: Option<i32>,
        enforced: Option<bool>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let assigned_at = chrono::Utc::now().to_rfc3339();
        let priority = priority.unwrap_or(100);
        let enforced = enforced.unwrap_or(true);

        sqlx::query(
            "INSERT INTO policy_assignments
             (id, policy_pack_id, target_type, target_id, priority, enforced, assigned_at, assigned_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(policy_pack_id)
        .bind(target_type)
        .bind(target_id)
        .bind(priority)
        .bind(enforced as i32)
        .bind(&assigned_at)
        .bind(assigned_by)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to assign policy: {}", e)))?;

        Ok(id)
    }

    /// Get policy assignments for a target
    pub async fn get_policy_assignments(
        &self,
        target_type: &str,
        target_id: Option<&str>,
    ) -> Result<Vec<PolicyAssignment>> {
        let assignments = if let Some(tid) = target_id {
            sqlx::query_as::<_, PolicyAssignment>(
                "SELECT * FROM policy_assignments
                 WHERE target_type = ? AND target_id = ?
                 ORDER BY priority DESC, assigned_at DESC",
            )
            .bind(target_type)
            .bind(tid)
            .fetch_all(&*self.pool())
            .await
        } else {
            sqlx::query_as::<_, PolicyAssignment>(
                "SELECT * FROM policy_assignments
                 WHERE target_type = ? AND target_id IS NULL
                 ORDER BY priority DESC, assigned_at DESC",
            )
            .bind(target_type)
            .fetch_all(&*self.pool())
            .await
        }
        .map_err(|e| AosError::Database(format!("Failed to get policy assignments: {}", e)))?;

        Ok(assignments)
    }

    /// Remove a policy assignment
    pub async fn remove_policy_assignment(&self, assignment_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM policy_assignments WHERE id = ?")
            .bind(assignment_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to remove policy assignment: {}", e))
            })?;

        Ok(())
    }

    // ========== Policy Violation Methods ==========

    /// Record a policy violation
    pub async fn record_policy_violation(
        &self,
        policy_pack_id: &str,
        policy_assignment_id: Option<&str>,
        violation_type: &str,
        severity: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        tenant_id: &str,
        violation_message: &str,
        violation_details_json: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let detected_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO policy_violations
             (id, policy_pack_id, policy_assignment_id, violation_type, severity,
              resource_type, resource_id, tenant_id, violation_message, violation_details_json, detected_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(policy_pack_id)
        .bind(policy_assignment_id)
        .bind(violation_type)
        .bind(severity)
        .bind(resource_type)
        .bind(resource_id)
        .bind(tenant_id)
        .bind(violation_message)
        .bind(violation_details_json)
        .bind(&detected_at)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record policy violation: {}", e)))?;

        Ok(id)
    }

    /// Get policy violations with filters
    pub async fn get_policy_violations(
        &self,
        tenant_id: Option<&str>,
        resource_type: Option<&str>,
        severity: Option<&str>,
        resolved: Option<bool>,
        limit: i64,
    ) -> Result<Vec<PolicyViolation>> {
        let mut query = String::from("SELECT * FROM policy_violations WHERE 1=1");
        let mut params: Vec<String> = Vec::new();

        if let Some(tid) = tenant_id {
            query.push_str(" AND tenant_id = ?");
            params.push(tid.to_string());
        }

        if let Some(rt) = resource_type {
            query.push_str(" AND resource_type = ?");
            params.push(rt.to_string());
        }

        if let Some(sev) = severity {
            query.push_str(" AND severity = ?");
            params.push(sev.to_string());
        }

        if let Some(r) = resolved {
            if r {
                query.push_str(" AND resolved_at IS NOT NULL");
            } else {
                query.push_str(" AND resolved_at IS NULL");
            }
        }

        query.push_str(" ORDER BY detected_at DESC LIMIT ?");
        params.push(limit.to_string());

        let mut q = sqlx::query_as::<_, PolicyViolation>(&query);
        for param in &params {
            q = q.bind(param);
        }

        let violations = q
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get policy violations: {}", e)))?;

        Ok(violations)
    }

    /// Resolve a policy violation
    pub async fn resolve_policy_violation(
        &self,
        violation_id: &str,
        resolved_by: &str,
        resolution_notes: Option<&str>,
    ) -> Result<()> {
        let resolved_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE policy_violations
             SET resolved_at = ?, resolved_by = ?, resolution_notes = ?
             WHERE id = ?",
        )
        .bind(&resolved_at)
        .bind(resolved_by)
        .bind(resolution_notes)
        .bind(violation_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to resolve policy violation: {}", e)))?;

        Ok(())
    }

    // ========== Compliance Score Methods ==========

    /// Store a compliance score
    pub async fn store_compliance_score(
        &self,
        target_type: &str,
        target_id: Option<&str>,
        policy_pack_id: Option<&str>,
        score: f64,
        total_checks: i32,
        passed_checks: i32,
        failed_checks: i32,
        critical_violations: i32,
        high_violations: i32,
        medium_violations: i32,
        low_violations: i32,
        period_start: Option<&str>,
        period_end: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let calculated_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO compliance_scores
             (id, target_type, target_id, policy_pack_id, score, total_checks, passed_checks,
              failed_checks, critical_violations, high_violations, medium_violations, low_violations,
              calculated_at, period_start, period_end)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(target_type)
        .bind(target_id)
        .bind(policy_pack_id)
        .bind(score)
        .bind(total_checks)
        .bind(passed_checks)
        .bind(failed_checks)
        .bind(critical_violations)
        .bind(high_violations)
        .bind(medium_violations)
        .bind(low_violations)
        .bind(&calculated_at)
        .bind(period_start)
        .bind(period_end)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to store compliance score: {}", e)))?;

        Ok(id)
    }

    /// Get latest compliance score for a target
    pub async fn get_compliance_score(
        &self,
        target_type: &str,
        target_id: Option<&str>,
        policy_pack_id: Option<&str>,
    ) -> Result<Option<ComplianceScore>> {
        let score = if let Some(ppid) = policy_pack_id {
            if let Some(tid) = target_id {
                sqlx::query_as::<_, ComplianceScore>(
                    "SELECT * FROM compliance_scores
                     WHERE target_type = ? AND target_id = ? AND policy_pack_id = ?
                     ORDER BY calculated_at DESC LIMIT 1",
                )
                .bind(target_type)
                .bind(tid)
                .bind(ppid)
                .fetch_optional(&*self.pool())
                .await
            } else {
                sqlx::query_as::<_, ComplianceScore>(
                    "SELECT * FROM compliance_scores
                     WHERE target_type = ? AND target_id IS NULL AND policy_pack_id = ?
                     ORDER BY calculated_at DESC LIMIT 1",
                )
                .bind(target_type)
                .bind(ppid)
                .fetch_optional(&*self.pool())
                .await
            }
        } else if let Some(tid) = target_id {
            sqlx::query_as::<_, ComplianceScore>(
                "SELECT * FROM compliance_scores
                 WHERE target_type = ? AND target_id = ? AND policy_pack_id IS NULL
                 ORDER BY calculated_at DESC LIMIT 1",
            )
            .bind(target_type)
            .bind(tid)
            .fetch_optional(&*self.pool())
            .await
        } else {
            sqlx::query_as::<_, ComplianceScore>(
                "SELECT * FROM compliance_scores
                 WHERE target_type = ? AND target_id IS NULL AND policy_pack_id IS NULL
                 ORDER BY calculated_at DESC LIMIT 1",
            )
            .bind(target_type)
            .fetch_optional(&*self.pool())
            .await
        }
        .map_err(|e| AosError::Database(format!("Failed to get compliance score: {}", e)))?;

        Ok(score)
    }

    /// Get policy assignments specifically for a stack
    pub async fn get_policy_assignments_for_stack(
        &self,
        stack_id: &str,
    ) -> Result<Vec<PolicyAssignment>> {
        let assignments = sqlx::query_as::<_, PolicyAssignment>(
            "SELECT * FROM policy_assignments
             WHERE target_type = 'stack' AND target_id = ?
             ORDER BY priority DESC, assigned_at DESC",
        )
        .bind(stack_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get stack policy assignments: {}", e))
        })?;

        Ok(assignments)
    }

    /// Calculate compliance summary for a stack
    ///
    /// Returns a StackComplianceData with overall score and category breakdowns
    pub async fn calculate_stack_compliance(
        &self,
        stack_id: &str,
        _tenant_id: &str,
    ) -> Result<StackComplianceData> {
        use std::collections::HashMap;

        // Get violations for this stack (unresolved) to calculate compliance
        let violations = sqlx::query_as::<_, PolicyViolation>(
            "SELECT pv.* FROM policy_violations pv
             JOIN policy_assignments pa ON pv.policy_assignment_id = pa.id
             WHERE pa.target_type = 'stack' AND pa.target_id = ?
             AND pv.resolved_at IS NULL
             ORDER BY pv.detected_at DESC",
        )
        .bind(stack_id)
        .fetch_all(&*self.pool())
        .await
        .unwrap_or_else(|_| vec![]);

        // Get policy assignments for the stack to count total policies
        let assignments = self.get_policy_assignments_for_stack(stack_id).await?;

        // Calculate scores by category
        // Categories: security, quality, compliance, performance
        let mut by_category: HashMap<String, CategoryComplianceData> = HashMap::new();

        // Initialize categories
        for cat in &["security", "quality", "compliance", "performance"] {
            by_category.insert(
                cat.to_string(),
                CategoryComplianceData {
                    score: 100.0,
                    passed: 0,
                    failed: 0,
                },
            );
        }

        // Count violations by severity and map to categories
        let mut total_violations = 0;
        let mut critical_violations = 0;
        let mut high_violations = 0;

        for v in &violations {
            total_violations += 1;
            match v.severity.as_str() {
                "critical" => critical_violations += 1,
                "high" => high_violations += 1,
                _ => {}
            }

            // Map violation resource_type to category
            let category = match v.resource_type.as_str() {
                "egress" | "isolation" | "secrets" => "security",
                "determinism" | "router" | "naming" => "quality",
                "evidence" | "telemetry" | "audit" => "compliance",
                "memory" | "latency" | "throughput" => "performance",
                _ => "compliance", // Default to compliance
            };

            if let Some(cat_score) = by_category.get_mut(category) {
                cat_score.failed += 1;
            }
        }

        // Calculate passed count based on assignments
        let total_checks = assignments.len() as i32;
        for cat_score in by_category.values_mut() {
            // Distribute total checks evenly across categories for simplicity
            let cat_total = total_checks / 4;
            cat_score.passed = (cat_total - cat_score.failed).max(0);
            // Calculate score: (passed / total) * 100
            if cat_total > 0 {
                cat_score.score = (cat_score.passed as f64 / cat_total as f64) * 100.0;
            }
        }

        // Calculate overall score
        // Formula: 100 - (critical * 25) - (high * 10) - (other * 2), min 0
        let overall_score = (100.0
            - (critical_violations as f64 * 25.0)
            - (high_violations as f64 * 10.0)
            - ((total_violations - critical_violations - high_violations) as f64 * 2.0))
            .max(0.0)
            .min(100.0);

        // Determine status
        let status = if overall_score >= 90.0 {
            "compliant"
        } else if overall_score >= 70.0 {
            "warning"
        } else {
            "non_compliant"
        };

        let now = chrono::Utc::now().to_rfc3339();

        Ok(StackComplianceData {
            overall_score,
            status: status.to_string(),
            by_category,
            last_calculated: now,
        })
    }

    /// Get recent policy violations for a stack (within the last N hours)
    pub async fn get_recent_stack_violations(
        &self,
        stack_id: &str,
        hours: i64,
    ) -> Result<Vec<PolicyViolation>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours);
        let cutoff_str = cutoff.to_rfc3339();

        let violations = sqlx::query_as::<_, PolicyViolation>(
            "SELECT pv.* FROM policy_violations pv
             JOIN policy_assignments pa ON pv.policy_assignment_id = pa.id
             WHERE pa.target_type = 'stack' AND pa.target_id = ?
             AND pv.detected_at >= ?
             ORDER BY pv.detected_at DESC
             LIMIT 50",
        )
        .bind(stack_id)
        .bind(&cutoff_str)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get recent stack violations: {}", e)))?;

        Ok(violations)
    }

    /// Get compliance scores over time for trending
    pub async fn get_compliance_trend(
        &self,
        target_type: &str,
        target_id: Option<&str>,
        policy_pack_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ComplianceScore>> {
        let scores = if let Some(ppid) = policy_pack_id {
            if let Some(tid) = target_id {
                sqlx::query_as::<_, ComplianceScore>(
                    "SELECT * FROM compliance_scores
                     WHERE target_type = ? AND target_id = ? AND policy_pack_id = ?
                     ORDER BY calculated_at DESC LIMIT ?",
                )
                .bind(target_type)
                .bind(tid)
                .bind(ppid)
                .bind(limit)
                .fetch_all(&*self.pool())
                .await
            } else {
                sqlx::query_as::<_, ComplianceScore>(
                    "SELECT * FROM compliance_scores
                     WHERE target_type = ? AND target_id IS NULL AND policy_pack_id = ?
                     ORDER BY calculated_at DESC LIMIT ?",
                )
                .bind(target_type)
                .bind(ppid)
                .bind(limit)
                .fetch_all(&*self.pool())
                .await
            }
        } else if let Some(tid) = target_id {
            sqlx::query_as::<_, ComplianceScore>(
                "SELECT * FROM compliance_scores
                 WHERE target_type = ? AND target_id = ? AND policy_pack_id IS NULL
                 ORDER BY calculated_at DESC LIMIT ?",
            )
            .bind(target_type)
            .bind(tid)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        } else {
            sqlx::query_as::<_, ComplianceScore>(
                "SELECT * FROM compliance_scores
                 WHERE target_type = ? AND target_id IS NULL AND policy_pack_id IS NULL
                 ORDER BY calculated_at DESC LIMIT ?",
            )
            .bind(target_type)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        }
        .map_err(|e| AosError::Database(format!("Failed to get compliance trend: {}", e)))?;

        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_policy_pack_storage() {
        let db = Db::new_in_memory().await.unwrap();

        let id = db
            .store_policy_pack(
                "cp-egress-001",
                "1.0",
                "egress",
                r#"{"mode": "deny_all"}"#,
                "sig:abc123",
                "pubkey:xyz789",
                "b3:hash123",
                "admin@example.com",
                Some("Egress policy for production"),
            )
            .await
            .unwrap();

        assert_eq!(id, "cp-egress-001");

        let pack = db.get_policy_pack(&id).await.unwrap().unwrap();
        assert_eq!(pack.policy_type, "egress");
        assert_eq!(pack.status, "draft");

        // Activate the policy
        db.activate_policy_pack(&id).await.unwrap();
        let pack = db.get_policy_pack(&id).await.unwrap().unwrap();
        assert_eq!(pack.status, "active");
    }

    #[tokio::test]
    async fn test_policy_assignment() {
        let db = Db::new_in_memory().await.unwrap();

        // Store a policy pack first
        let policy_id = db
            .store_policy_pack(
                "cp-naming-001",
                "1.0",
                "naming",
                r#"{"require_semantic": true}"#,
                "sig:abc",
                "pubkey:xyz",
                "b3:hash",
                "admin@example.com",
                None,
            )
            .await
            .unwrap();

        // Assign to tenant
        let assignment_id = db
            .assign_policy(
                &policy_id,
                "tenant",
                Some("tenant-a"),
                "admin@example.com",
                Some(200),
                Some(true),
            )
            .await
            .unwrap();

        assert!(!assignment_id.is_empty());

        // Get assignments for tenant
        let assignments = db
            .get_policy_assignments("tenant", Some("tenant-a"))
            .await
            .unwrap();

        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].policy_pack_id, policy_id);
    }

    #[tokio::test]
    async fn test_policy_violation_tracking() {
        let db = Db::new_in_memory().await.unwrap();

        // Store policy pack
        let policy_id = db
            .store_policy_pack(
                "cp-determinism-001",
                "1.0",
                "determinism",
                r#"{"require_hkdf": true}"#,
                "sig:abc",
                "pubkey:xyz",
                "b3:hash",
                "admin@example.com",
                None,
            )
            .await
            .unwrap();

        // Record violation
        let violation_id = db
            .record_policy_violation(
                &policy_id,
                None,
                "determinism",
                "high",
                "adapter",
                Some("adapter-xyz"),
                "tenant-a",
                "Non-deterministic RNG detected",
                Some(r#"{"method": "thread_rng"}"#),
            )
            .await
            .unwrap();

        assert!(!violation_id.is_empty());

        // Query violations
        let violations = db
            .get_policy_violations(Some("tenant-a"), None, None, Some(false), 10)
            .await
            .unwrap();

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, "high");

        // Resolve violation
        db.resolve_policy_violation(
            &violation_id,
            "operator@example.com",
            Some("Fixed RNG seeding"),
        )
        .await
        .unwrap();

        let violations = db
            .get_policy_violations(Some("tenant-a"), None, None, Some(false), 10)
            .await
            .unwrap();

        assert_eq!(violations.len(), 0);
    }

    #[tokio::test]
    async fn test_compliance_scoring() {
        let db = Db::new_in_memory().await.unwrap();

        // Store compliance score
        let score_id = db
            .store_compliance_score(
                "tenant",
                Some("tenant-a"),
                None,
                0.95,
                100,
                95,
                5,
                0,
                2,
                3,
                0,
                None,
                None,
            )
            .await
            .unwrap();

        assert!(!score_id.is_empty());

        // Get latest score
        let score = db
            .get_compliance_score("tenant", Some("tenant-a"), None)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(score.score, 0.95);
        assert_eq!(score.total_checks, 100);
        assert_eq!(score.passed_checks, 95);
    }
}
