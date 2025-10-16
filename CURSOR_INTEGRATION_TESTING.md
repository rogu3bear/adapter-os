# Testing Cursor Integration

## Start AdapterOS Server
```bash
cargo run --bin adapteros-server -- --config configs/cp.toml
```

## Test Models Endpoint
```bash
curl http://localhost:8080/v1/models
```

## Test Chat Completions with API Key
```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer adapteros-local" \
  -d '{
    "model": "adapteros-qwen2.5-7b",
    "messages": [
      {"role": "user", "content": "How do I implement authentication in Rust?"}
    ],
    "max_tokens": 1000
  }'
```

## Configure Cursor

Add to Cursor settings.json:
```json
{
  "cursor.customModels": [
    {
      "name": "AdapterOS Local",
      "baseUrl": "http://localhost:8080",
      "apiKey": "adapteros-local",
      "model": "adapteros-qwen2.5-7b",
      "maxTokens": 4000,
      "temperature": 0.7
    }
  ]
}
```
