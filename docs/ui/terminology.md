# UI Terminology Standards

This document defines the canonical terminology used throughout the AdapterOS UI. Consistency in naming helps users build mental models and reduces confusion.

## Terminology Mapping

| Old/Variant Label | New Canonical Label | Where Used |
|-------------------|---------------------|------------|
| Flight Recorder | **Runs** | Page header, navigation, StartMenu |
| Trace Viewer | **Trace** | Run Detail tab |
| Proof | **Receipt** | Run Detail tab, chat links |
| Verification | **Receipt** | Run Detail tab |
| Diagnostics | **Runs** | API references in UI |
| Inference History | **Runs** | Navigation descriptions |

## Core Concepts

### Run
A single inference execution with full provenance tracking. Every inference generates a Run that can be inspected and verified.

- **Run ID**: Unique identifier for the run (internal database ID)
- **Trace ID**: Correlation ID used across the system (use this for deep links)

### Receipt
Cryptographic proof of what happened during a run. Includes:
- Request hash
- Output hash
- Timing breakdown
- Hardware attestation

Terminology note: Always use "Receipt" not "Proof" or "Verification".

### Trace
Detailed timeline and breakdown of a run's execution phases. Shows:
- Latency metrics
- Token routing decisions
- Adapter usage

### Worker
A runtime inference process managed in `/workers` and `/workers/:id`.

### Stack
A configured combination of model, adapters, and policy. Not yet captured in Run Detail (shows as "Unknown").

### Policy
Enforcement rules applied during inference. Not yet captured in Run Detail (shows as "Unknown").

## Page Headers

| Route | Header Title | Subheader |
|-------|--------------|-----------|
| `/` | Dashboard | System overview and quick actions |
| `/chat` | Chat | Use the system to reason, generate, and run inference |
| `/runs` | Runs | Inference run history and diagnostics |
| `/runs/:id` | Run Detail | [trace_id truncated] |
| `/audit` | Audit | Immutable audit log with hash chain verification |
| `/routing` | Routing Debug | Inspect and manage how requests are routed |
| `/diff` | Run Diff | Compare diagnostic runs and launch into the Run Detail diff tab |

## Tab Labels (Run Detail)

| Tab | Label | Description |
|-----|-------|-------------|
| Overview | Overview | Summary, status, timing, adapters |
| Trace | Trace | Full trace visualization |
| Receipt | Receipt | Cryptographic verification |
| Routing | Routing | K-sparse routing decisions |
| Tokens | Tokens | Token accounting and cache stats |
| Diff | Diff | Compare with another run |
| Events | Events | Raw event stream |

## Run Status Labels

Use these consistently across run views:

| Status | Badge Variant | Context |
|--------|---------------|---------|
| running | Default (blue) | Run in progress |
| completed | Success (green) | Run finished successfully |
| failed | Destructive (red) | Run encountered error |
| cancelled | Warning (yellow) | Run was cancelled |

## Worker Status Semantics

Use these status terms consistently for worker lifecycle UX:

| Status | Meaning | Badge Variant | Terminal |
|--------|---------|---------------|----------|
| healthy | Ready and accepting inference requests. | Success (green) | No |
| draining | Rejects new requests while in-flight work finishes. | Warning (yellow) | No |
| stopped | Clean shutdown completed. | Destructive (red) | Yes |
| error | Canonical terminal failure status. | Destructive (red) | Yes |
| crashed | Legacy compatibility alias for terminal failure (treated as `error`). | Destructive (red) | Yes |
| failed | Legacy compatibility terminal failure string (handled like `error`/`crashed`). | Destructive (red) | Yes |

## Action Labels

| Action | Canonical Verb | Example |
|--------|----------------|---------|
| Starting inference | "Run" | "New Run" button |
| Viewing receipt | "Verify" | "Verify Receipt" link |
| Opening full page | "View" | "View Run" link |
| Comparing runs | "Diff" | "Compare Runs" button |
| Copying ID | "Copy" | "Copy Run ID" action |

## Navigation Terminology

In StartMenu and navigation:

| Module | Label | NOT |
|--------|-------|-----|
| Runs | "Runs" | "Flight Recorder", "Diagnostics" |
| Audit | "Audit" | "Audit Log", "Audit Trail" |
| Operate | "Operate" | "Operations", "Monitoring" |

## Deep Link Format

Use trace_id for all deep links to runs:
- `/runs/{trace_id}` - Overview (default)
- `/runs/{trace_id}?tab=trace` - Trace tab
- `/runs/{trace_id}?tab=receipt` - Receipt tab
- `/runs/{trace_id}?tab=diff&compare={other_trace_id}` - Diff comparison
