# Phase 4: OpenAI API Completeness - Research

**Researched:** 2026-02-24
**Domain:** OpenAI compatibility shim, API contracts, request/response behavior
**Confidence:** HIGH

## Summary

The OpenAI compatibility surface is already centralized and production-wired through `crates/adapteros-server-api/src/handlers/openai_compat.rs` with routes exposed in `crates/adapteros-server-api/src/routes/mod.rs` (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/models`). Usage fields are present on completion responses, and there is existing compatibility coverage for streaming chunk format, embeddings schema, and model listing.

The largest gaps relative to roadmap requirements are parameter and feature completeness: `response_format/json_schema`, `tools/tool_choice`, and missing OpenAI generation fields (`seed`, `stop`, `frequency_penalty`, `presence_penalty`, `logprobs`) are not represented in the current OpenAI request structs. Error shaping exists via `OpenAiErrorResponse` but currently fixes `error.type` to `invalid_request_error`, which is insufficient for full parity.

**Primary recommendation:** Keep all implementation in the existing `openai_compat` module and split execution into two plans: behavior completeness first (`API-01..04`), then OpenAPI drift/error-format hardening (`API-05..06`) with compatibility regression tests.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Extend the existing OpenAI shim (`handlers/openai_compat.rs`), no parallel handler path.
- Keep current OpenAI routes and expand in place.
- Route all behavior through current inference core interfaces, adapting at boundary.
- Plan 04-01 covers `API-01..API-04`; Plan 04-02 covers `API-05..API-06`.
- `response_format` and `tools/tool_choice` must be supported or rejected with OpenAI-formatted errors.
- Missing parameters (`seed`, `stop`, `frequency_penalty`, `presence_penalty`, `logprobs`) must be accepted and influence behavior or have explicit compatibility handling.
- OpenAPI is generated from utoipa (`routes::ApiDoc`) and exported via existing tool; drift must be eliminated.
- OpenAI error envelope must be consistent: `error.message`, `error.type`, optional `error.code`, optional `error.param`.

### Claude's Discretion
- Internal mapping abstractions for OpenAI fields.
- Native vs fallback behavior for partially supported parameters.
- Concrete test layout and fixture strategy.

### Deferred Ideas (OUT OF SCOPE)
- New non-roadmap endpoint families and SDK efforts.
- Performance tuning beyond correctness baseline.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| API-01 | Structured output via `response_format` (JSON schema enforcement) | `OpenAiChatCompletionsRequest` currently lacks `response_format`; no `json_schema` enforcement path exists in `openai_compat.rs`. Gap is explicit and localized. |
| API-02 | Tool/function calling via `tools` and `tool_choice` | OpenAI request/response types in `openai_compat.rs` do not currently include `tools`/`tool_choice` or tool-call output structures. Gap is explicit and localized. |
| API-03 | Complete usage fields in completion responses | `OpenAiUsage` exists with `prompt_tokens`, `completion_tokens`, `total_tokens`; responses populate usage in chat/completions handlers. Need consistency checks across all success paths. |
| API-04 | Missing parameters supported (`seed`, `stop`, `frequency_penalty`, `presence_penalty`, `logprobs`) | These parameters are absent from OpenAI request structs; some underlying inference types have adjacent fields (for example `seed` in api-types inference), indicating feasible mapping path. |
| API-05 | OpenAPI spec in sync with code | OpenAPI is centrally generated via utoipa (`routes::ApiDoc`) and export tool (`src/bin/export-openapi.rs`), enabling deterministic drift checks once schema fields are expanded. |
| API-06 | Error responses follow OpenAI format | `OpenAiErrorResponse` exists, and mapping helper `map_adapteros_error_to_openai` already routes many errors; currently `error.type` is fixed to `invalid_request_error`, so typing granularity is incomplete. |
</phase_requirements>

## Existing Native Patterns

### Pattern 1: Single OpenAI Compatibility Boundary
- Request/response shape definitions and endpoint logic are co-located in `handlers/openai_compat.rs`.
- Route wiring points directly to those handlers from `routes/mod.rs`.
- Benefit: one compatibility boundary to extend without route or ownership duplication.

### Pattern 2: AdapterOS Error -> OpenAI Envelope Translation
- Existing helper (`map_adapteros_error_to_openai`) converts `ErrorResponse` into OpenAI-style envelope.
- Benefit: expand classification without rewriting endpoint-level error handling everywhere.

### Pattern 3: OpenAPI as Generated Artifact
- `ApiDoc` macro in routes and `export-openapi` tool provide stable contract generation.
- Benefit: drift detection can be automated as a targeted check for Phase 04 acceptance.

### Pattern 4: OpenAI-Specific Test Files
- Existing tests in `crates/adapteros-server-api/tests/` (`openai_chat_completions_streaming.rs`, `openai_embeddings_tests.rs`, `openai_models_list_test.rs`) already encode compatibility style.
- Benefit: extend these suites to avoid introducing parallel compatibility tests.

## Gaps and Risks

### Gap A: Structured Output Enforcement Design
- Risk: accepting `response_format` without schema-constrained generation yields false compatibility claims.
- Mitigation: define explicit enforcement strategy (strict validation with deterministic failure path) in 04-01.

### Gap B: Tool Calling Output Semantics
- Risk: partial tool payload shape can break LangChain/CrewAI parsing despite passing basic tests.
- Mitigation: add fixture-based contract tests for tool call request+response envelopes in 04-01 and 04-02.

### Gap C: Parameter Acceptance vs Behavioral Effect
- Risk: adding fields without behavior influence violates success criterion #4.
- Mitigation: add targeted behavior assertions (seed reproducibility, stop truncation, penalties/logprobs effects or explicit unsupported error).

### Gap D: OpenAPI Drift During Rapid Iteration
- Risk: endpoint behavior changes and schema annotations diverge.
- Mitigation: include export-openapi diff check in 04-02 verification.

### Gap E: Over-Broad Error Type Mapping
- Risk: all failures labeled `invalid_request_error` reduce client interoperability.
- Mitigation: map status/code classes to OpenAI-compatible error types and add regression tests for multiple error classes.

## Minimal Scope Recommendation

1. Add missing OpenAI request fields and response variants in existing `openai_compat.rs` types.
2. Map fields into existing inference requests using current conversion paths and explicit guardrails.
3. Extend OpenAI-focused tests in existing files and add only missing focused suites.
4. Regenerate and verify OpenAPI output from existing toolchain.
5. Lock error envelope compatibility with deterministic tests.

## Verification Baseline for Phase 04 Plans

- Targeted server-api test runs for OpenAI compatibility files.
- OpenAPI export command from existing `export-openapi` binary, followed by drift check.
- At least one concrete request/response validation per requirement (`API-01..API-06`).

---

*Phase: 04-openai-api-completeness*
*Research completed: 2026-02-24*
