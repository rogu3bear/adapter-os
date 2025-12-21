# OpenCode (OpenAI-Compatible) Integration

AdapterOS exposes a minimal OpenAI Chat Completions–compatible endpoint at:

- `POST /v1/chat/completions`

## Base URL

Most OpenAI-compatible clients expect an API base that already includes `/v1`.

- Base URL: `http://localhost:8080/v1`
- Chat completions path: `/chat/completions`

If your client instead expects a root base URL and appends `/v1` internally, use:

- Base URL: `http://localhost:8080`

## Authentication

### Dev (no-auth)

For local development, you can disable auth (debug builds only):

- `make dev-no-auth`
- or `AOS_DEV_NO_AUTH=1 cargo run --bin adapteros-server`

In no-auth mode, requests succeed without an `Authorization` header (some clients still require a dummy API key in their config).

### Auth enabled (API key)

AdapterOS accepts OpenAI-style API key headers:

- `Authorization: Bearer <api_key>`

## Quick curl smoke test

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer <api_key>' \
  -d '{
    "model": "qwen2.5-7b",
    "messages": [
      {"role": "system", "content": "You are AdapterOS."},
      {"role": "user", "content": "Say hello in one short sentence."}
    ]
  }' | jq .
```

Expected shape (abridged):

```json
{
  "choices": [
    {
      "message": { "role": "assistant", "content": "..." }
    }
  ],
  "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 }
}
```

## Notes

- `stream=true` is not supported on `/v1/chat/completions` yet; use `POST /v1/infer/stream` for SSE streaming.

