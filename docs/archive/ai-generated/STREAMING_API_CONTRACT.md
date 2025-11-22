# Streaming Inference API Contract

**Document Purpose:** Complete specification for the streaming inference endpoint
**Last Updated:** 2025-11-19
**Status:** Verified and Documented

---

## Overview

AdapterOS provides a Server-Sent Events (SSE) based streaming inference API for real-time token-by-token inference responses. The API follows OpenAI's chat completion format for compatibility with existing clients.

**Key Features:**
- SSE (Server-Sent Events) streaming protocol
- Chat completion compatible format
- Token-by-token streaming responses
- Configurable inference parameters (temperature, max_tokens, top_p)
- Support for adapter stacks and telemetry correlation
- Automatic keep-alive messages (15-second intervals)
- Non-streaming fallback endpoint

---

## Endpoints

### 1. Streaming Inference (SSE)

**Endpoint:** `/v1/chat/completions`
**Method:** `POST`
**Protocol:** HTTP/1.1 with Server-Sent Events
**Response Type:** `text/event-stream`

### 2. Non-Streaming Completion (Fallback)

**Endpoint:** `/v1/completions`
**Method:** `POST`
**Response Type:** `application/json`

---

## Request Format

### Endpoint: POST `/v1/chat/completions`

#### Headers Required
```
Content-Type: application/json
Accept: text/event-stream  (recommended for streaming clients)
```

#### Request Body (JSON)

```json
{
  "prompt": "string (required)",
  "model": "string (optional)",
  "max_tokens": 512,
  "temperature": 0.7,
  "top_p": null,
  "stop": [],
  "stream": true,
  "adapter_stack": "stack-name (optional)",
  "stack_id": "uuid (optional, for telemetry)",
  "stack_version": 1
}
```

#### Field Descriptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `prompt` | string | required | Input prompt or user message for inference |
| `model` | string | "adapteros" | Model identifier (informational) |
| `max_tokens` | number | 512 | Maximum tokens to generate |
| `temperature` | number | 0.7 | Sampling temperature (0.0-2.0) |
| `top_p` | number | null | Nucleus sampling parameter (optional) |
| `stop` | string[] | [] | Stop sequences to end generation |
| `stream` | boolean | true | Enable streaming response (always true for this endpoint) |
| `adapter_stack` | string | null | Name of adapter stack to use (optional) |
| `stack_id` | string | null | UUID for telemetry correlation (PRD-03) |
| `stack_version` | number | null | Stack version for correlation tracking |

#### Example Request

```bash
curl -X POST http://localhost:8080/api/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{
    "prompt": "Explain quantum computing in simple terms",
    "model": "adapteros",
    "max_tokens": 200,
    "temperature": 0.7,
    "stream": true
  }' \
  -N
```

---

## Response Format

### Streaming Response (SSE Format)

The response uses Server-Sent Events (SSE) format where each event is a JSON chunk.

#### Event Structure

```
data: {json-object}\n\n
```

Each message is prefixed with `data: ` and terminated with two newlines.

### Response Event Types

#### 1. Token Event (Content Chunk)

Sent for each token generated:

```json
{
  "id": "chatcmpl-550e8400-e29b-41d4-a716-446655440000",
  "object": "chat.completion.chunk",
  "created": 1234567890,
  "model": "adapteros",
  "system_fingerprint": null,
  "choices": [
    {
      "index": 0,
      "delta": {
        "role": null,
        "content": "Hello"
      },
      "finish_reason": null
    }
  ]
}
```

**Fields:**
- `id`: Unique identifier for this completion session
- `object`: Always `"chat.completion.chunk"` for streaming
- `created`: Unix timestamp
- `model`: Model name (from request or default "adapteros")
- `system_fingerprint`: Determinism tracking (optional)
- `choices[0].delta.content`: The token/word generated
- `finish_reason`: `null` for content chunks

#### 2. Completion Event

Sent when generation finishes:

```json
{
  "id": "chatcmpl-550e8400-e29b-41d4-a716-446655440000",
  "object": "chat.completion.chunk",
  "created": 1234567890,
  "model": "adapteros",
  "system_fingerprint": null,
  "choices": [
    {
      "index": 0,
      "delta": {
        "role": null,
        "content": null
      },
      "finish_reason": "stop"
    }
  ]
}
```

**Fields:**
- `finish_reason`: `"stop"` indicates completion
- `delta.content`: `null` (no new content)

#### 3. Error Event

Sent when an error occurs:

```json
{
  "error": {
    "message": "Inference failed: model not loaded",
    "type": "inference_error",
    "code": null
  }
}
```

**Fields:**
- `error.message`: Human-readable error message
- `error.type`: Error category (`"inference_error"`)
- `error.code`: Optional error code (null)

#### 4. Keep-Alive Event

Sent every 15 seconds to keep connection alive:

```
: keep-alive

```

(Comment-style SSE event)

### Example SSE Response Stream

```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1700000000,"model":"adapteros","choices":[{"index":0,"delta":{"content":"The"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1700000000,"model":"adapteros","choices":[{"index":0,"delta":{"content":" weather"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1700000000,"model":"adapteros","choices":[{"index":0,"delta":{"content":" is"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1700000000,"model":"adapteros","choices":[{"index":0,"delta":{"content":" nice"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1700000000,"model":"adapteros","choices":[{"index":0,"delta":{"content":" today"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1700000000,"model":"adapteros","choices":[{"index":0,"delta":{"finish_reason":"stop"}}]}
```

---

## Non-Streaming Fallback: `/v1/completions`

For clients that don't support streaming, use the non-streaming endpoint.

### Request

```json
{
  "prompt": "string (required)",
  "model": "string (optional)",
  "max_tokens": 512,
  "temperature": 0.7,
  "top_p": null,
  "stop": [],
  "stream": false
}
```

### Response

```json
{
  "id": "chatcmpl-550e8400-e29b-41d4-a716-446655440000",
  "object": "chat.completion",
  "created": 1234567890,
  "model": "adapteros",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "The complete generated response text..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 12,
    "completion_tokens": 45,
    "total_tokens": 57
  }
}
```

---

## Authentication & Authorization

### Required Headers

```
Authorization: Bearer {jwt_token}
```

(If auth is configured)

### Permissions Required

- `InferenceExecute` permission (if RBAC enabled)
- Tenant isolation enforced automatically

### Error Responses

#### Unauthorized (401)
```json
{
  "error": "unauthorized",
  "details": "Invalid or missing authentication token"
}
```

#### Forbidden (403)
```json
{
  "error": "policy violation",
  "details": "Permission denied: InferenceExecute required"
}
```

---

## Implementation Details

### Streaming Mechanism

**Current Implementation:** Simulated streaming (as of v0.01)

The backend generates the complete response first, then simulates streaming by:
1. Splitting response text by whitespace (word-by-word)
2. Sending each word as a separate SSE event
3. Adding 10ms delay between events to simulate streaming

**Limitations:**
- No time-to-first-token improvement
- Full generation happens before streaming begins
- Client disconnects not detected until after generation

**Future Improvement:**
True token-by-token streaming at the kernel level for:
- Faster time-to-first-token
- Better resource utilization
- Client disconnect detection

### Keep-Alive Mechanism

- **Interval:** Every 15 seconds
- **Format:** SSE comment: `: keep-alive`
- **Purpose:** Prevents connection timeout on slow/stalled streams

### Error Handling

**Client Disconnect Detection:**
- Detected when attempting to send data to closed channel
- Silently handled; generation can continue without penalty
- No resource waste but user won't receive output

### Worker Integration

Calls `Worker.infer()` with:
```rust
InferenceRequest {
    cpid: uuid::Uuid::new_v4().to_string(),
    prompt: request.prompt,
    max_tokens: request.max_tokens,
    require_evidence: false,
    request_type: RequestType::Normal,
    stack_id: request.stack_id,
    stack_version: request.stack_version,
}
```

---

## Client Implementation Examples

### JavaScript/TypeScript (Fetch API)

```typescript
async function streamInference(prompt: string) {
  const response = await fetch('/api/v1/chat/completions', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Accept': 'text/event-stream',
    },
    body: JSON.stringify({
      prompt,
      max_tokens: 200,
      temperature: 0.7,
      stream: true,
    }),
  });

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value);
    const lines = buffer.split('\n');
    buffer = lines.pop() || '';

    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const jsonStr = line.slice(6);
        if (jsonStr.trim() === '') continue;

        try {
          const chunk = JSON.parse(jsonStr);
          const content = chunk.choices[0]?.delta?.content;
          if (content) {
            console.log(content); // Process token
          }
          if (chunk.choices[0]?.finish_reason === 'stop') {
            console.log('Generation complete');
            return;
          }
        } catch (e) {
          console.error('Parse error:', e);
        }
      }
    }
  }
}

streamInference('Hello, how are you?');
```

### Python (requests library)

```python
import requests
import json

def stream_inference(prompt: str):
    url = 'http://localhost:8080/api/v1/chat/completions'
    headers = {
        'Content-Type': 'application/json',
        'Accept': 'text/event-stream'
    }
    data = {
        'prompt': prompt,
        'max_tokens': 200,
        'temperature': 0.7,
        'stream': True
    }

    response = requests.post(url, json=data, headers=headers, stream=True)

    for line in response.iter_lines():
        if line.startswith(b'data: '):
            json_str = line[6:].decode('utf-8')
            if json_str.strip() == '':
                continue

            try:
                chunk = json.loads(json_str)
                content = chunk.get('choices', [{}])[0].get('delta', {}).get('content')
                if content:
                    print(content, end='', flush=True)

                finish_reason = chunk.get('choices', [{}])[0].get('finish_reason')
                if finish_reason == 'stop':
                    print('\nGeneration complete')
                    break
            except json.JSONDecodeError as e:
                print(f'Parse error: {e}')

stream_inference('Hello, how are you?')
```

### cURL Command

```bash
curl -X POST http://localhost:8080/api/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{
    "prompt": "Explain quantum computing",
    "max_tokens": 200,
    "temperature": 0.7,
    "stream": true
  }' \
  -N  # Unbuffered output
```

---

## Error Codes & Messages

### Common Errors

| Status | Code | Message | Cause |
|--------|------|---------|-------|
| 400 | BAD_REQUEST | Invalid request format | Malformed JSON or missing required fields |
| 401 | UNAUTHORIZED | Invalid authentication | Missing or expired token |
| 403 | FORBIDDEN | Policy violation | Insufficient permissions |
| 500 | INFERENCE_ERROR | Inference failed: {reason} | Worker error, model not loaded |
| 500 | WORKER_ERROR | Worker communication failed | Backend worker unavailable |

### Example Error Response

```json
{
  "error": {
    "message": "Inference failed: model not loaded",
    "type": "inference_error",
    "code": null
  }
}
```

---

## Performance Characteristics

### Response Time

- **Time to first byte (TTFB):** ~100-500ms (generation time)
- **Token latency:** ~10ms per token (simulated)
- **Total time:** ~100ms + (token_count × 10ms)

### Resource Usage

- **Memory:** ~100MB per concurrent request
- **CPU:** Varies with model and prompt size
- **Concurrency:** Limited by worker capacity (typically 1-4)

### Limits

- **Max prompt length:** 50,000 characters (50KB)
- **Max prompt bytes:** 100,000 bytes
- **Max tokens:** Configurable per adapter
- **Connection timeout:** 15 seconds (keep-alive prevents)

---

## Telemetry & Observability

### Tracing Fields

The streaming handler emits structured logs:

```rust
info!(
    "Starting streaming inference: prompt_len={}, max_tokens={}",
    request.prompt.len(),
    request.max_tokens
);

debug!(
    "Running inference: prompt_len={}, max_tokens={}",
    request.prompt.len(),
    request.max_tokens
);

debug!("Streaming token: {}", content);

info!("Streaming complete: {}", finish_reason);
```

### Telemetry Events

Generated during inference:
- `inference.started` - Inference began
- `inference.completed` - Inference finished
- `inference.error` - Error occurred
- `client.disconnected` - Client disconnected

### Stack Correlation (PRD-03)

Pass `stack_id` and `stack_version` to correlate streaming responses with:
- Stack versioning
- Multi-tenant tracking
- Audit trails

---

## Integration Points

### From TypeScript/UI

**File:** `/Users/star/Dev/aos/ui/src/api/client.ts`

```typescript
async infer(
  data: InferRequest,
  options: RequestInit = {},
  skipRetry: boolean = false,
  cancelToken?: AbortSignal
): Promise<InferResponse>
```

**Usage:**
```typescript
const response = await apiClient.infer(
  { prompt: "Hello", max_tokens: 100 },
  {},
  false,
  signal
);
```

### From Rust Backend

**File:** `/Users/star/Dev/aos/crates/adapteros-api/src/streaming.rs`

```rust
pub async fn streaming_inference_handler<K: FusedKernels + Send + Sync + 'static>(
    State(state): State<Arc<ApiState<K>>>,
    Json(request): Json<StreamingInferenceRequest>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>
```

---

## Testing

### Unit Tests (Rust)

**File:** `/Users/star/Dev/aos/crates/adapteros-api/src/streaming.rs`

```rust
#[test]
fn test_streaming_request_defaults() {
    let req = StreamingInferenceRequest { ... };
    assert_eq!(req.max_tokens, 512);
    assert!((req.temperature - 0.7).abs() < 0.01);
    assert!(req.stream);
}

#[test]
fn test_streaming_chunk_serialization() {
    let chunk = StreamingChunk { ... };
    let json = serde_json::to_string(&chunk).expect("Failed to serialize");
    assert!(json.contains("Hello"));
    assert!(json.contains("chat.completion.chunk"));
}
```

### Integration Tests

**Test Script:** `/tmp/test_streaming.sh`

```bash
# Verify streaming response format
curl -X POST http://localhost:8080/api/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{"prompt":"Hello","stream":true}' \
  -N
```

---

## Compatibility

### OpenAI API Compatibility

- Endpoint format: Compatible with OpenAI's `/v1/chat/completions`
- Request schema: Subset of OpenAI's ChatCompletionRequest
- Response format: Compatible with OpenAI's streaming format
- **Differences:** No `messages` array (uses single `prompt` instead)

### Client Libraries

Compatible with clients supporting OpenAI API:
- OpenAI Python library (with adapter)
- OpenAI JavaScript library (with adapter)
- LangChain (with streaming support)
- Custom SSE clients

---

## Known Limitations & Future Work

### Current Limitations

1. **Simulated Streaming:** Full response generated before streaming
2. **No Message Format:** Single prompt string instead of messages array
3. **Single Response:** Only one choice (index 0)
4. **No Usage Estimates:** Token counts are word-based estimates

### Planned Improvements

1. **True Token Streaming:** Kernel-level token-by-token generation
2. **Messages Support:** OpenAI-compatible messages format
3. **Multiple Choices:** Support for multiple generation branches
4. **Token Counting:** Accurate token counting via tokenizer
5. **Client Disconnect Detection:** Early detection and cancellation
6. **Rate Limiting:** Per-user/tenant rate limits
7. **Caching:** Response caching for identical prompts

---

## References

- **Source Code:** `/Users/star/Dev/aos/crates/adapteros-api/src/streaming.rs`
- **Routes:** `/Users/star/Dev/aos/crates/adapteros-api/src/lib.rs` (lines 110-113)
- **Types:** `/Users/star/Dev/aos/crates/adapteros-api/src/streaming.rs` (lines 28-138)
- **Worker Integration:** `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/lib.rs`
- **UI Integration:** `/Users/star/Dev/aos/ui/src/api/client.ts` (line 906)
- **OpenAI API Ref:** https://platform.openai.com/docs/api-reference/chat/create

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-11-19 | Initial documentation of streaming API contract |

