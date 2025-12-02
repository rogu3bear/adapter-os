# Inference Pipeline Test Guide

This guide provides web-based tools (curl) to test the AdapterOS inference pipeline.

## Prerequisites

1. **Start the server:**
   ```bash
   ./start
   # or
   cargo run --release -p adapteros-server-api
   ```

2. **Start the worker (required for inference):**
   ```bash
   AOS_DEV_SKIP_METALLIB_CHECK=1 cargo run -p adapteros-lora-worker --bin aos-worker -- \
     --manifest manifests/qwen7b-mlx.yaml \
     --model-path ./var/model-cache/models/qwen2.5-7b-instruct-bf16 \
     --uds-path ./var/run/worker.sock
   ```

## Quick Test Commands

### 1. Health Check (Public)
```bash
curl http://localhost:8080/healthz
```

Expected: `{"status":"healthy",...}`

### 2. Readiness Check (Public)
```bash
curl http://localhost:8080/readyz
```

Expected: `{"status":"ready",...}`

### 3. API Metadata (Public)
```bash
curl http://localhost:8080/v1/meta
```

### 4. Authentication

**Bootstrap admin (first time only):**
```bash
curl -X POST http://localhost:8080/v1/auth/bootstrap \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}'
```

**Login:**
```bash
curl -X POST http://localhost:8080/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}'
```

Save the token from the response for subsequent requests.

### 5. Standard Inference

```bash
TOKEN="your-token-here"

curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "prompt": "Write a hello world function in Rust",
    "max_tokens": 100,
    "temperature": 0.7
  }'
```

### 6. Streaming Inference (SSE)

```bash
curl -N -X POST http://localhost:8080/v1/infer/stream \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "prompt": "Explain async/await in Rust",
    "max_tokens": 200,
    "stream": true
  }'
```

### 7. Batch Inference

```bash
curl -X POST http://localhost:8080/v1/infer/batch \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "requests": [
      {"prompt": "What is Rust?", "max_tokens": 50},
      {"prompt": "Explain ownership", "max_tokens": 50}
    ]
  }'
```

## Automated Test Script

Run the comprehensive test suite:

```bash
./test_inference_pipeline.sh
```

This script tests:
- Health endpoints
- Authentication
- Standard inference
- Streaming inference
- Batch inference

## Expected Responses

### Successful Inference Response
```json
{
  "schema_version": "1.0",
  "id": "uuid-here",
  "text": "Generated text...",
  "tokens": [],
  "tokens_generated": 0,
  "finish_reason": "stop",
  "latency_ms": 0,
  "adapters_used": [],
  "trace": {
    "adapters_used": [],
    "router_decisions": [],
    "latency_ms": 0
  }
}
```

### Error Responses

**401 Unauthorized:**
```json
{
  "error": "unauthorized",
  "message": "Authentication required"
}
```

**503 Service Unavailable (Worker not ready):**
```json
{
  "error": "service_unavailable",
  "message": "worker not available",
  "details": "..."
}
```

## Troubleshooting

1. **Connection refused:** Server not running
2. **503 Service Unavailable:** Worker not started or not ready
3. **401 Unauthorized:** Need to login and get token
4. **Timeout:** Worker may be loading model (check worker logs)

## Testing with Browser

You can also test via the Swagger UI:

```bash
open http://localhost:8080/swagger-ui
```

Navigate to `/v1/infer` endpoint and use "Try it out" feature.

---

MLNavigator Inc 2025-01-27.

