# UDS/API/SSR Contract

This document defines the reliability-first contract for the server control plane, worker UDS transport, and SSR shell behavior.

## Canonical Ownership

- HTTP API route ownership: `crates/adapteros-server-api/src/routes/mod.rs`
- SSR shell rendering ownership: `crates/adapteros-server/src/assets.rs` and `crates/adapteros-server/src/ssr.rs`
- Worker UDS protocol ownership: `crates/adapteros-lora-worker/src/uds_server.rs`
- Shared inference transport contract ownership: `crates/adapteros-inference-contract`

## API Namespace Policy

- Canonical external namespace: `/v1/*`
- Legacy namespace: `/api/v1/*` is compatibility-only
- Compatibility deprecation start: `2026-02-21T00:00:00Z`
- Compatibility sunset target: `2026-08-31T00:00:00Z`

Behavior:
- Server route definitions stay canonical (`/v1/*`).
- `/api/v1/*` is rewritten to `/v1/*` by compatibility middleware.
- Compatibility responses include deprecation/sunset headers.

## UDS Transport Contract

Canonical UDS paths:
- Inference: `/inference`
- Inference cancel prefix: `/inference/cancel`
- Inference resume prefix: `/inference/resume`

Compatibility-only UDS path:
- Legacy inference alias: `/api/v1/infer`

Notes:
- Worker keeps temporary acceptance of `/api/v1/infer` during migration.
- New client defaults should use `/inference`.

## SSR Response Contract

Required SSR shell contract:
- SSR outlet marker: `<!--AOS_SSR_OUTLET-->`
- Legacy outlet fallback markers supported: `<div id="aos-app-root"></div>`, `<div id="root"></div>`
- Response header semantics:
  - `X-AOS-SSR: 1` when server-rendered route markup is injected.
  - `X-AOS-SSR: 0` when static shell fallback is returned.

## Drift Prevention

- Canonical route map generator: `scripts/dev/generate_route_map.sh`
- OpenAPI source of truth remains `ApiDoc::openapi()` and `docs/api/openapi.json`.
