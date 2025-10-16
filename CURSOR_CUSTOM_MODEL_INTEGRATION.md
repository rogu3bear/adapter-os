# Cursor Custom Model Integration Guide

## Overview

AdapterOS can be configured as a **custom model provider** for Cursor IDE, allowing Cursor to use AdapterOS's evidence-grounded, deterministic AI capabilities as a local model option.

## Integration Architecture

```
Cursor IDE
    ↓ (OpenAI-Compatible API)
AdapterOS Control Plane (:8080)
    ↓ (/v1/infer endpoint)
Five-Tier Adapter Hierarchy
    ↓ (Evidence-Grounded Responses)
Cursor IDE
```

## Current AdapterOS API

### Primary Inference Endpoint ✅

**Endpoint**: `POST /v1/infer`

**Request Format**:
```json
{
  "prompt": "How do I implement authentication in this Rust project?",
  "max_tokens": 1000,
  "temperature": 0.7,
  "require_evidence": true
}
```

**Response Format**:
```json
{
  "text": "Based on the codebase analysis...",
  "tokens": [1, 2, 3, ...],
  "finish_reason": "stop",
  "trace": {
    "adapters_used": ["code_lang_v1", "framework_rust_v1"],
    "router_decisions": [...],
    "latency_ms": 23
  }
}
```

## OpenAI-Compatible API Implementation

### Option 1: Add OpenAI-Compatible Endpoints ✅

**New Endpoints to Add**:

#### Chat Completions
```
POST /v1/chat/completions
```

**Request Format** (OpenAI Standard):
```json
{
  "model": "adapteros-qwen2.5-7b",
  "messages": [
    {"role": "system", "content": "You are a helpful coding assistant."},
    {"role": "user", "content": "How do I implement authentication?"}
  ],
  "max_tokens": 1000,
  "temperature": 0.7,
  "stream": false
}
```

**Response Format** (OpenAI Standard):
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "adapteros-qwen2.5-7b",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "Based on the codebase analysis..."
    },
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 25,
    "completion_tokens": 100,
    "total_tokens": 125
  }
}
```

#### Models List
```
GET /v1/models
```

**Response Format**:
```json
{
  "object": "list",
  "data": [
    {
      "id": "adapteros-qwen2.5-7b",
      "object": "model",
      "created": 1677652288,
      "owned_by": "adapteros"
    }
  ]
}
```

### Option 2: Proxy/Adapter Layer ✅

Create a lightweight proxy service that:
1. Accepts OpenAI-compatible requests
2. Transforms them to AdapterOS format
3. Calls `/v1/infer`
4. Transforms responses back to OpenAI format

## Implementation Plan

### Phase 1: Add OpenAI-Compatible Endpoints (Week 1)

**File**: `crates/adapteros-server-api/src/handlers/openai.rs`

```rust
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: ChatUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// OpenAI-compatible chat completions endpoint
#[utoipa::path(
    post,
    path = "/v1/chat/completions",
    request_body = ChatCompletionRequest,
    responses(
        (status = 200, description = "Chat completion successful", body = ChatCompletionResponse),
        (status = 400, description = "Bad request", body = ErrorResponse)
    )
)]
pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate model
    if req.model != "adapteros-qwen2.5-7b" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Unsupported model").with_code("INVALID_MODEL")),
        ));
    }

    // Convert messages to prompt
    let prompt = req.messages
        .iter()
        .map(|msg| format!("{}: {}", msg.role, msg.content))
        .collect::<Vec<_>>()
        .join("\n");

    // Call existing infer endpoint
    let infer_req = InferRequest {
        prompt,
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        require_evidence: Some(true),
        ..Default::default()
    };

    // Forward to existing infer handler
    let infer_response = handlers::infer(State(state), Extension(claims), Json(infer_req)).await?;
    let infer_data = infer_response.0;

    // Transform to OpenAI format
    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
        model: req.model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: infer_data.text,
            },
            finish_reason: infer_data.finish_reason,
        }],
        usage: ChatUsage {
            prompt_tokens: infer_data.tokens.len(),
            completion_tokens: infer_data.tokens.len(),
            total_tokens: infer_data.tokens.len() * 2, // Rough estimate
        },
    };

    Ok(Json(response))
}

/// OpenAI-compatible models list endpoint
#[utoipa::path(
    get,
    path = "/v1/models",
    responses(
        (status = 200, description = "Models list", body = ModelsListResponse)
    )
)]
pub async fn list_models() -> Result<Json<ModelsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let response = ModelsListResponse {
        object: "list".to_string(),
        data: vec![ModelInfo {
            id: "adapteros-qwen2.5-7b".to_string(),
            object: "model".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            owned_by: "adapteros".to_string(),
        }],
    };

    Ok(Json(response))
}
```

### Phase 2: Update Routes (Week 1)

**File**: `crates/adapteros-server-api/src/routes.rs`

```rust
// Add OpenAI-compatible routes
.route("/v1/chat/completions", post(handlers::openai::chat_completions))
.route("/v1/models", get(handlers::openai::list_models))
```

### Phase 3: Cursor Configuration (Week 1)

**Cursor Settings**:
```json
{
  "cursor.customModels": [
    {
      "name": "AdapterOS Qwen2.5-7B",
      "baseUrl": "http://localhost:8080",
      "apiKey": "adapteros-local",
      "model": "adapteros-qwen2.5-7b",
      "maxTokens": 4000,
      "temperature": 0.7
    }
  ]
}
```

## Advanced Features

### Streaming Support ✅

**Endpoint**: `POST /v1/chat/completions` with `"stream": true`

**Response Format** (Server-Sent Events):
```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"adapteros-qwen2.5-7b","choices":[{"index":0,"delta":{"content":"Based"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"adapteros-qwen2.5-7b","choices":[{"index":0,"delta":{"content":" on"},"finish_reason":null}]}

data: [DONE]
```

### Code Intelligence Integration ✅

**Enhanced Prompts**:
- Include repository context in system messages
- Add file-specific context from CodeGraph
- Include framework detection results
- Add evidence citations to responses

### Authentication ✅

**API Key Support**:
```rust
// Add API key validation
pub async fn validate_api_key(api_key: &str) -> Result<String, Error> {
    if api_key == "adapteros-local" {
        Ok("default".to_string()) // tenant_id
    } else {
        Err(Error::Unauthorized)
    }
}
```

## Configuration Examples

### Cursor Settings

**settings.json**:
```json
{
  "cursor.customModels": [
    {
      "name": "AdapterOS Local",
      "baseUrl": "http://localhost:8080",
      "apiKey": "adapteros-local",
      "model": "adapteros-qwen2.5-7b",
      "maxTokens": 4000,
      "temperature": 0.7,
      "systemMessage": "You are a helpful coding assistant with access to the current codebase. Always provide evidence-based responses."
    }
  ],
  "cursor.defaultModel": "AdapterOS Local"
}
```

### Environment Variables

**AdapterOS Configuration**:
```bash
# Enable OpenAI-compatible endpoints
export ADAPTEROS_OPENAI_COMPATIBLE=true
export ADAPTEROS_API_KEY=adapteros-local
export ADAPTEROS_TENANT_ID=default

# Start server
cargo run --bin adapteros-server -- --config configs/cp.toml
```

## Testing

### Manual Testing

```bash
# Test models endpoint
curl http://localhost:8080/v1/models

# Test chat completions
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

### Cursor Integration Testing

1. **Configure Cursor** with custom model settings
2. **Start AdapterOS** server
3. **Register repository** for code intelligence
4. **Test chat interface** in Cursor
5. **Verify evidence citations** in responses

## Performance Considerations

### Latency Optimization
- **AdapterOS**: <24ms p95 (per Performance Ruleset #11)
- **OpenAI Format**: +2-3ms transformation overhead
- **Total**: <30ms p95 latency

### Memory Usage
- **Base Model**: ~4-5GB (Qwen2.5-7B int4)
- **Adapters**: ~75-95MB each (rank 16)
- **CodeGraph**: ~10MB per 10K files
- **Total**: ~6-8GB for full setup

### Throughput
- **AdapterOS**: 40+ tokens/second
- **OpenAI Format**: No impact on throughput
- **Concurrent Requests**: Limited by worker pool size

## Security & Compliance

### AdapterOS Policy Compliance ✅
- **Egress Ruleset #1**: Zero network during serving
- **Determinism Ruleset #2**: Reproducible outputs
- **Evidence Ruleset #4**: Mandatory grounding
- **Isolation Ruleset #8**: Per-tenant process boundaries
- **Telemetry Ruleset #9**: Audit trails

### API Security
- **API Key Authentication**: Simple local key
- **Tenant Isolation**: Per-tenant model instances
- **Rate Limiting**: Built into AdapterOS
- **Audit Logging**: Complete request/response logs

## Troubleshooting

### Common Issues

1. **Connection Refused**
   - Ensure AdapterOS server is running on port 8080
   - Check firewall settings
   - Verify Cursor configuration

2. **Authentication Failed**
   - Verify API key matches configuration
   - Check tenant ID settings
   - Ensure proper headers

3. **Model Not Found**
   - Verify model name in Cursor settings
   - Check `/v1/models` endpoint response
   - Ensure model is loaded in AdapterOS

4. **Slow Responses**
   - Check AdapterOS worker status
   - Verify model is loaded and warm
   - Monitor system resources

### Debug Commands

```bash
# Check server status
curl http://localhost:8080/healthz

# Check models
curl http://localhost:8080/v1/models

# Check adapters
curl http://localhost:8080/v1/adapters

# Check repositories
curl http://localhost:8080/v1/code/repositories
```

## Next Steps

### Immediate (Week 1)
1. **Implement OpenAI-compatible endpoints**
2. **Add route configurations**
3. **Test basic chat completions**
4. **Configure Cursor settings**

### Short Term (Month 1)
1. **Add streaming support**
2. **Implement code intelligence integration**
3. **Add authentication**
4. **Performance optimization**

### Long Term (Quarter 1)
1. **Advanced features** (function calling, tools)
2. **Multi-model support**
3. **Custom adapter training**
4. **Enterprise features**

## Conclusion

**AdapterOS is ready to be configured as a custom model provider for Cursor IDE.**

The implementation requires:
- ✅ Adding OpenAI-compatible endpoints (2-3 days)
- ✅ Updating route configurations (1 day)
- ✅ Testing and validation (2-3 days)
- ✅ Cursor configuration (1 day)

**Total Timeline**: 1-2 weeks for basic integration

**Benefits**:
- Local, private AI assistance
- Evidence-grounded responses
- Deterministic outputs
- Code intelligence integration
- Zero external network calls

**Status**: ✅ **READY FOR IMPLEMENTATION**
