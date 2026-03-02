# Phase 4: OpenAI API Completeness - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Complete the OpenAI-compatible API surface so agent frameworks can use adapterOS without adapters or custom shims. This phase adds structured output, tool/function calling, missing generation parameters, usage accounting consistency, and contract/error-shape alignment. Core model runtime behavior, determinism internals, and observability pipelines remain outside this phase.

</domain>

<decisions>
## Implementation Decisions

### Compatibility-First Extension Strategy
- Extend the existing `crates/adapteros-server-api/src/handlers/openai_compat.rs` path; do not introduce parallel OpenAI handlers.
- Keep `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, and `/v1/models` as the compatibility entrypoints and expand request/response fields in place.
- Preserve current inference core wiring through `handlers::inference::infer` and `InferenceRequestInternal`; adapt inputs at the boundary.

### Requirement Sequencing
- Plan 04-01 focuses on runtime request/response behavior: `API-01`, `API-02`, `API-03`, `API-04`.
- Plan 04-02 focuses on contract/error correctness and compatibility verification: `API-05`, `API-06` plus regression validation.
- Each plan must leave the OpenAI shim in a usable intermediate state (no contract-breaking half-steps).

### Structured Output and Tooling Scope
- `response_format` (including `json_schema`) must be accepted and enforced for supported modes.
- `tools` and `tool_choice` must be accepted and surfaced in OpenAI-compatible response shapes (including tool call payloads where applicable).
- Unsupported combinations must fail with OpenAI-style error envelopes, not adapterOS-native error shapes.

### Parameter Completeness and Usage Truthfulness
- Accept and map `seed`, `stop`, `frequency_penalty`, `presence_penalty`, and `logprobs` through the OpenAI shim to underlying inference behavior or explicit, documented fallback.
- Ensure `usage.prompt_tokens`, `usage.completion_tokens`, and `usage.total_tokens` are always coherent for successful completion responses.
- Keep token usage computation deterministic with current estimation/tokenizer pathways; no speculative accounting modes.

### Contract and Drift Control
- OpenAPI definitions (utoipa paths/schemas via `routes::ApiDoc`) must reflect implemented request/response fields.
- Use existing OpenAPI generation flow (`crates/adapteros-server-api/src/bin/export-openapi.rs`) as the source of truth for drift checks.
- Error bodies for OpenAI endpoints must consistently follow `{ error: { message, type, code?, param? } }`.

### Agent/Execution Policy for This Phase
- Planning artifact edits are tightly coupled; use a single agent stream for these docs to avoid parallel-structure duplication.
- If implementation later splits into separable workstreams (runtime behavior vs contract tooling), use agent teams with strict file ownership boundaries.

### Claude's Discretion
- Exact internal abstractions for translating OpenAI fields into `InferRequest`/`InferenceRequestInternal`.
- Whether to support some parameters natively vs accept-and-noop with explicit compatibility notes (must be test-covered).
- Test fixture design for tool-calling and structured-output scenarios.

</decisions>

<specifics>
## Specific Ideas

- Existing ownership is centralized in `crates/adapteros-server-api/src/handlers/openai_compat.rs` and route wiring in `crates/adapteros-server-api/src/routes/mod.rs`.
- Existing OpenAI tests already cover streaming/chat format, embeddings format, and models listing; extend these suites instead of creating parallel test harnesses.
- Requirements for this phase: `API-01`, `API-02`, `API-03`, `API-04`, `API-05`, `API-06`.
- Success criteria anchor points:
- `response_format.json_schema` validity enforcement
- `tools`/`tool_choice` compatibility payloads
- complete and accurate `usage` fields
- missing parameter acceptance and behavior influence
- OpenAPI and runtime parity with OpenAI error envelope conformance

</specifics>

<deferred>
## Deferred Ideas

- New endpoint families beyond current roadmap scope (for example, fully separate Responses API surface) unless required by MVP requirements.
- Broader agent framework SDK packaging and language-specific helper libraries (already out of scope per project constraints).
- Performance optimization of constrained decoding beyond correctness baseline (defer to observability/runtime hardening phase if needed).

</deferred>

---

*Phase: 04-openai-api-completeness*
*Context gathered: 2026-02-24*
