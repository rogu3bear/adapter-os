# Streaming API Quick Reference

**Quick lookup guide for the streaming inference endpoint**

---

## Endpoint

```
POST /v1/chat/completions
```

---

## Headers

```
Content-Type: application/json
Accept: text/event-stream
```

---

## Request

```json
{
  "prompt": "required: input text",
  "max_tokens": 512,
  "temperature": 0.7,
  "stream": true
}
```

---

## Response (SSE Events)

### Token Event
```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion.chunk",
  "choices": [{
    "delta": { "content": "word" },
    "finish_reason": null
  }]
}
```

### Completion Event
```json
{
  "choices": [{
    "delta": { "content": null },
    "finish_reason": "stop"
  }]
}
```

---

## cURL Test

```bash
curl -X POST http://localhost:8080/api/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{"prompt":"Hello","stream":true}' -N
```

---

## JavaScript Client

```typescript
const response = await fetch('/api/v1/chat/completions', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ prompt: "Hello", stream: true })
});

for await (const chunk of response.body) {
  const line = new TextDecoder().decode(chunk);
  if (line.startsWith('data: ')) {
    const event = JSON.parse(line.slice(6));
    console.log(event.choices[0].delta.content);
  }
}
```

---

## Key Fields

| Field | Type | Default | Required |
|-------|------|---------|----------|
| prompt | string | - | YES |
| max_tokens | number | 512 | no |
| temperature | number | 0.7 | no |
| model | string | "adapteros" | no |
| adapter_stack | string | null | no |
| stream | boolean | true | no |
| stack_id | string | null | no |
| stack_version | number | null | no |

---

## Event Types

1. **Token** - `delta.content` has text, `finish_reason: null`
2. **Done** - `delta.content: null`, `finish_reason: "stop"`
3. **Error** - JSON with `error` object
4. **Keep-Alive** - Comment line every 15s (`: keep-alive`)

---

## Alternatives

### Non-Streaming
```
POST /v1/completions
```
Returns single JSON response (no SSE)

---

## Source Files

- **Implementation:** `/Users/star/Dev/aos/crates/adapteros-api/src/streaming.rs`
- **Routes:** `/Users/star/Dev/aos/crates/adapteros-api/src/lib.rs:113`
- **Full Docs:** `/Users/star/Dev/aos/STREAMING_API_CONTRACT.md`
- **Summary:** `/Users/star/Dev/aos/STREAMING_API_SUMMARY.md`

---

## Status

✓ Endpoint operational
✓ SSE streaming working
✓ Error handling implemented
✓ Keep-alive configured
✓ UI integrated
⏳ True token-level streaming (planned)

