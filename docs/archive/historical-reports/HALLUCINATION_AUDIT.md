# Hallucination Audit and Reconciliation Report

## Status: ✅ Complete

This document provides a deterministic audit of hallucinations found within the codebase. For this report, a "hallucination" is defined as a deviation from the project's verified sources of truth, such as its deterministic execution guidelines, established architectural patterns, and central configuration system.

All identified deviations have been flagged and resolved with deterministic patches.

---

### Deviation 1: Hardcoded Tenant ID (H1)

-   **Location**: `crates/adapteros-orchestrator/src/code_jobs.rs`
-   **Deviation**: The `execute_commit_delta_job` function used a hardcoded `"default"` tenant ID when querying the database for a repository. This is non-deterministic as it ignores the tenant context provided by the API caller.
-   **Patch**:
    1.  The `CommitDeltaJob` struct was extended to include a `tenant_id: String` field【1†crates/adapteros-orchestrator/src/code_jobs.rs†90-90】.
    2.  The API handler at `crates/adapteros-server-api/src/handlers/code.rs` was updated to correctly propagate the `tenant_id` from the API request into the `CommitDeltaJob` struct【2†crates/adapteros-server-api/src/handlers/code.rs†537-538】.
    3.  The `execute_commit_delta_job` function was modified to use this `job.tenant_id` for the database query, making the job context-aware and deterministic【3†crates/adapteros-orchestrator/src/code_jobs.rs†178-178】.

---

### Deviation 2: Non-Deterministic Task Spawning (H2)

-   **Location**: `crates/adapteros-server-api/src/handlers/code.rs`
-   **Deviation**: The `create_commit_delta` API handler used `tokio::spawn` to launch the background job. This violates the project's explicit guideline to use its own deterministic executor for all core logic.
-   **Patch**: The call was replaced with `spawn_deterministic`, ensuring the commit delta job is executed within the controlled, deterministic environment provided by the project's executor framework【4†crates/adapteros-server-api/src/handlers/code.rs†542-542】.

---

### Deviation 3: Incorrect `DiffAnalyzer` Instantiation (H3)

-   **Location**: `crates/adapteros-orchestrator/src/code_jobs.rs`
-   **Deviation**: The `generate_training_data_from_cdp` helper function was instantiating the `DiffAnalyzer` with a logical `repo_id` (e.g., `"my/repo"`) instead of a physical filesystem path. This would cause the underlying `git` command to fail at runtime.
-   **Patch**:
    1.  The function signature for `generate_training_data_from_cdp` was changed to accept the physical `repo_path: &Path`【5†crates/adapteros-orchestrator/src/code_jobs.rs†245-245】.
    2.  The `execute_commit_delta_job` function, which has access to the correct path, was updated to pass it to the helper function【6†crates/adapteros-orchestrator/src/code_jobs.rs†189-189】.
    3.  The helper was refactored to create a single `DiffAnalyzer` instance with the correct path, ensuring all subsequent `git` operations would succeed【7†crates/adapteros-orchestrator/src/code_jobs.rs†252-252】.

---

## Conclusion

The deterministic hallucination check is complete. All identified deviations from project guidelines have been explicitly flagged and patched. The affected components now correctly adhere to the project's standards for configuration, deterministic execution, and context propagation.

### References

【1】 `crates/adapteros-orchestrator/src/code_jobs.rs` (Line 90)
【2】 `crates/adapteros-server-api/src/handlers/code.rs` (Lines 537-538)
【3】 `crates/adapteros-orchestrator/src/code_jobs.rs` (Line 178)
【4】 `crates/adapteros-server-api/src/handlers/code.rs` (Line 542)
【5】 `crates/adapteros-orchestrator/src/code_jobs.rs` (Line 245)
【6】 `crates/adapteros-orchestrator/src/code_jobs.rs` (Line 189)
【7】 `crates/adapteros-orchestrator/src/code_jobs.rs` (Line 252)


