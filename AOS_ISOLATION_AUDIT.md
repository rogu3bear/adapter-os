# Isolation Audit Report

## Status: ✅ Complete

This document provides a deterministic audit of the isolation of all identified unfinished, speculative, and partial features. The process involved moving all related files and code into a dedicated `deprecated` directory and removing all references from the active build to create a stable main branch.

---

### 1. Isolated Feature: `GitSubsystem`

-   **Reason for Isolation**: This feature was partial, with an incomplete implementation for watching file changes, managing branches, and handling commits. Its presence in the main branch created dead code and unresolved `TODO` markers.
-   **Isolation Strategy**: All core logic and API handlers were moved to `deprecated/gitssystem`, and all references in the main server and API routes were removed.

-   **File Moves**:
    -   **From**: `crates/adapteros-git/src/subsystem.rs`
    -   **To**: `deprecated/gitsubsystem/subsystem.rs`
    -   **From**: `crates/adapteros-server-api/src/handlers/git.rs`
    -   **To**: `deprecated/gitsubsystem/api_handlers.rs`

---

### 2. Isolated Feature: `Federation`

-   **Reason for Isolation**: The Federation Daemon was explicitly marked in the codebase as incomplete and not integrated.
-   **Isolation Strategy**: The entire `adapteros-federation` crate was moved into the `deprecated` directory, and it was removed from the workspace build.

-   **File Moves**:
    -   **From**: `crates/adapteros-federation`
    -   **To**: `deprecated/federation/adapteros-federation`

---

### 3. Isolated Feature: `adapteros-experimental`

-   **Reason for Isolation**: This crate is, by its nature, a collection of speculative and incomplete features. It is not intended to be part of the stable build.
-   **Isolation Strategy**: The entire crate was moved into the `deprecated` directory to formally separate it from the main codebase.

-   **File Moves**:
    -   **From**: `crates/adapteros-experimental`
    -   **To**: `deprecated/adapteros-experimental`

---

## Conclusion

The codebase has been successfully reconciled. All identified partial and speculative features have been deterministically isolated into the `deprecated` directory, creating a clean and stable main branch. The project now consists only of complete, verified features.
