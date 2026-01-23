# adapterOS Reference API Curl Cookbook

Copy/paste-friendly `curl` commands for the reference control plane API (default `http://localhost:8080`).

## Setup

```bash
export AOS_BASE_URL="${AOS_BASE_URL:-http://localhost:8080}"

# If auth is enabled, include this header on protected routes:
#   -H "Authorization: Bearer $AOS_TOKEN"
#
# Reference mode often runs with auth bypass enabled (debug builds), in which case you can omit it.

# Deterministic seed IDs (from `seeds/pilot_reference.sqlite.sql`)
export AOS_TENANT_ID="00000000-0000-4000-8000-000000000001"
export AOS_MODEL_ID="00000000-0000-4000-8000-000000000003"
export AOS_REPO_ID="00000000-0000-4000-8000-000000000004"
export AOS_ADAPTER_VERSION_ID="00000000-0000-4000-8000-000000000005"
export AOS_TRAINING_JOB_ID="00000000-0000-4000-8000-000000000006"
export AOS_STACK_ID="00000000-0000-4000-8000-000000000007"
```

## Healthz / Readyz

Routes:
- `GET /healthz`
- `GET /readyz`

```bash
curl -sS "$AOS_BASE_URL/healthz"
curl -sS "$AOS_BASE_URL/readyz"
```

## List Models

Route: `GET /v1/models`

```bash
curl -sS "$AOS_BASE_URL/v1/models"
```

## List Adapters / Repos / Versions

Routes:
- `GET /v1/adapters`
- `GET /v1/adapter-repositories`
- `GET /v1/adapter-repositories/{repo_id}/versions`
- `GET /v1/adapter-versions/{version_id}`

```bash
curl -sS "$AOS_BASE_URL/v1/adapters"

curl -sS "$AOS_BASE_URL/v1/adapter-repositories"
curl -sS "$AOS_BASE_URL/v1/adapter-repositories/$AOS_REPO_ID/versions"
curl -sS "$AOS_BASE_URL/v1/adapter-versions/$AOS_ADAPTER_VERSION_ID"
```

## Training Jobs (List)

Route: `GET /v1/training/jobs`

```bash
curl -sS "$AOS_BASE_URL/v1/training/jobs"
```

## Minimal Inference

Route: `POST /v1/infer`

```bash
curl -sS -X POST "$AOS_BASE_URL/v1/infer" \
  -H "Content-Type: application/json" \
  -d "{\"prompt\":\"Hello from adapterOS reference\",\"stack_id\":\"$AOS_STACK_ID\",\"max_tokens\":32}"
```
