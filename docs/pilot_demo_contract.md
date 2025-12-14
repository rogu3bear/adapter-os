# Pilot Demo Contract

## URLs / Ports

- API base URL: `http://localhost:8080/api` (TCP `8080`)
- UI URL: `http://localhost:3200` (TCP `3200`)

## Readiness

- `GET http://localhost:8080/api/healthz` → `200 OK`
- `GET http://localhost:8080/api/readyz` → `200 OK` when ready; `503` otherwise

## Script Environment Variables

- Ports: `AOS_SERVER_PORT=8080`, `AOS_UI_PORT=3200`, `API_PORT=8080`, `UI_PORT=3200`
- Timeouts: `HEALTH_TIMEOUT=180` (s), `HEALTH_INTERVAL=5` (s), `START_TIMEOUT=60` (s), `CURL_TIMEOUT=10` (s), `WAIT_ON_TIMEOUT=180000` (ms), `WAIT_ON_INTERVAL=1000` (ms)
- DB path: `DB_PATH=var/aos-cp.sqlite3`, `AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3`, `DATABASE_URL=sqlite://var/aos-cp.sqlite3`

## Seeded Entity UUIDs (Fixed)

- tenant: `00000000-0000-4000-8000-000000000001`
- user: `00000000-0000-4000-8000-000000000002`
- model: `00000000-0000-4000-8000-000000000003`
- repo: `00000000-0000-4000-8000-000000000004`
- adapter_version: `00000000-0000-4000-8000-000000000005`
- training_job: `00000000-0000-4000-8000-000000000006`
- stack: `00000000-0000-4000-8000-000000000007`

## Schema Compatibility (Sessions)

- Do **not** rename session tables for the demo (canonical tables stay as-is, e.g. `chat_sessions`, `chat_messages`).
- If there is a mismatch, add an additive compatibility table or view (preferred: `VIEW`) instead of renaming (e.g. `CREATE VIEW expected_name AS SELECT * FROM canonical_name;`).
