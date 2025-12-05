# Auth & Tenant UX Spec

## Goals and current pains
- Design around F1: fragile browser sessions (split-origin dev, cookies, silent/expired tokens → 401 loops/perceived logouts).
- Design around F2: confusing tenant context (wrong/blank tenant, unclear active tenant, missing picker).
- Design around F3: weak failure UX (generic errors, no distinction among bad creds/disabled/missing tenant).
- Success in 6 months: “boring” auth (no recurring tickets), stable SSO, clear tenant indicator, key/config rotation without UI breakage, single login across Build/Run/Observe/Verify, click budget ≈ one page + one submit.

## Personas and scope
- Builder (Build cluster), Operator (Run/Observe/Verify), Owner/Security (Verify/Security), Viewer (read-only), Dev (optional/internal).
- Roles: admin (all tenant surfaces), operator (run/monitor; no user mgmt/billing/high-risk security), owner/security (testing/golden/replay/security/audit; no low-level infra unless granted), viewer (read-only, no mutations), dev (access to dev-only surfaces).

## Session and token model
- Browser transport: HttpOnly secure cookie for refresh/session; short-lived access JWT (tenant id, role claims, session id) used server-side; JS should not read tokens.
- Lifetimes: access token short (minutes); refresh/session 8–12h max, idle 1–2h, extend via silent refresh while tab active.
- Cookie posture: SameSite=Lax now; allow upgrade to None; Secure in prod; handle split-origin dev by setting explicit domain/secure flags per env.
- Single session across clusters (Build/Run/Observe/Verify); no segmented sessions.
- 401 handling: attempt one silent refresh; on failure show banner “session expired—sign in again,” redirect to login, then return to original route.
- Deep links: preserve target through login + tenant selection; fail clearly if tenant mismatch/unauthorized.
- Revocation: per-user and per-tenant session kill; global by signing-key rotation + refresh invalidation.

## Tenant UX
- Active tenant: show name + icon/initials in header/nav badge; include in breadcrumbs where possible.
- Tenant picker: after login if >1 tenant; single-tenant users auto-enter. Tenant switcher in header forces reload of tenant-scoped data and updates access token with selected tenant id.
- One active tenant per session; users can switch manually, not automatically. If deep link targets another tenant, prompt to switch or show “not available in current tenant.”

## Flows and routes (v1)
- Support: login, logout, invite, password reset (local), OIDC SSO (Okta/Entra/generic). Magic link optional later.
- Auth-required: all app surfaces (Build/Run/Observe/Verify). Public: marketing, docs, status/health, landing.
- Default landing after login: `/dashboard` (role-aware); medium-term remember last cluster/page.
- Partial access: only login + tenant picker pre-auth; no app data without auth.

## Error taxonomy and UX states
- Invalid credentials: generic “Invalid email or password.”
- Account locked/disabled: explicit “Your account is locked/disabled. Contact an administrator.”
- Missing tenant membership: “You’re signed in but have no tenant access. Ask an admin to grant access.”
- Missing role within tenant: “You have no role in this tenant. Request access from an admin.”
- Session expired: banner/toast + redirect to login, then back to prior route.
- Stale/invalid invite: “This invite is no longer valid” + “Request new invite” action/mailto.
- IdP/SSO error: surface IdP error in a user-safe way; log detail server-side.
- Offline/back-end unreachable: show retry guidance, avoid silent loops.

## Dev bootstrap and local auth
- Dev bootstrap endpoint `/v1/dev/bootstrap` (debug/flag-gated) creates system tenant + admin (admin+dev roles) with simple dev creds behind env flag.
- Local auth mode: enabled for dev/small installs; can be disabled for hardened environments. UI shows DEV badge/env hint when in dev mode.
- Dev routes reuse same auth; dev-only surfaces are role-gated (dev or admin), not a separate auth path.

## Observability and health
- Metrics: login success/failure by reason (bad creds, locked, IdP error), source (local/SSO), per-tenant when known; active sessions per tenant; MFA enroll/fail.
- Logs/events: login_success, login_failure, logout, user_created, user_invited, invite_accepted, role_added/removed, tenant_added/removed_to_user, mfa_enabled/disabled, session_revoked.
- Health check `/v1/auth/health`: verify signing keys loaded, DB reachable, IdP config sanity; response is “ok” or coarse error code without secrets. Optionally polled by UI/ops.

## Testing outline
- Unit: token parse/verify, cookie settings, tenant claim handling, error mapping.
- Integration: login/logout/refresh, bootstrap happy path, locked/disabled responses, tenant selection claims.
- E2E: login→/dashboard, deep link through login (/replay/:id), tenant switch behavior, session expiry + refresh flow, invite acceptance.
- Smoke (staging): can log in, reach `/dashboard`, refresh works, health check ok.

MLNavigator Inc 2025-12-05.

