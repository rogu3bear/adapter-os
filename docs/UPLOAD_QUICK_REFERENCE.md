# Upload API - Quick Reference Card

**For complete guide, see:** [API_UPLOAD_GUIDE.md](API_UPLOAD_GUIDE.md)
**For troubleshooting, see:** [UPLOAD_TROUBLESHOOTING.md](UPLOAD_TROUBLESHOOTING.md)

---

## One-Liner Examples

### cURL

```bash
# Minimal upload
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@adapter.aos" \
  -F "name=My Adapter"

# Complete upload with all fields
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@adapter.aos" \
  -F "name=My Adapter" \
  -F "tier=persistent" \
  -F "category=code" \
  -F "rank=16" \
  -F "alpha=8.0"

# With progress bar
curl -# -X POST ... -F "file=@adapter.aos"

# Save response to file
curl -X POST ... -F "file=@adapter.aos" > response.json
```

### Python

```python
import requests

token = "your_jwt_token"
with open('adapter.aos', 'rb') as f:
    response = requests.post(
        'http://localhost:8080/v1/adapters/upload-aos',
        headers={'Authorization': f'Bearer {token}'},
        files={'file': f, 'name': (None, 'My Adapter')}
    )
print(response.json()['adapter_id'])
```

### Node.js

```javascript
const fs = require('fs');
const FormData = require('form-data');
const fetch = require('node-fetch');

const form = new FormData();
form.append('file', fs.createReadStream('adapter.aos'));
form.append('name', 'My Adapter');

const response = await fetch(
  'http://localhost:8080/v1/adapters/upload-aos',
  {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${token}` },
    body: form
  }
);
const data = await response.json();
console.log(data.adapter_id);
```

### Go

```go
package main
import (
    "bytes"
    "mime/multipart"
    "net/http"
    "os"
)

func upload(token, filePath string) {
    body := &bytes.Buffer{}
    writer := multipart.NewWriter(body)
    file, _ := os.Open(filePath)
    filePart, _ := writer.CreateFormFile("file", "adapter.aos")
    io.Copy(filePart, file)
    writer.WriteField("name", "My Adapter")
    writer.Close()

    req, _ := http.NewRequest("POST", "http://localhost:8080/v1/adapters/upload-aos", body)
    req.Header.Set("Authorization", "Bearer "+token)
    req.Header.Set("Content-Type", writer.FormDataContentType())
    http.DefaultClient.Do(req)
}
```

---

## Field Reference

| Field | Required | Type | Max Length | Example |
|-------|----------|------|-----------|---------|
| file | Yes | binary | 1GB | adapter.aos |
| name | No | string | 256 | "Code Review Adapter" |
| description | No | string | - | "For code review tasks" |
| tier | No | string | - | ephemeral, warm, persistent |
| category | No | string | - | general, code, text, vision, audio |
| scope | No | string | - | general, public, private, tenant |
| rank | No | int | - | 1-512 (default: 1) |
| alpha | No | float | - | 0.0-100.0 (default: 1.0) |

---

## Status Codes

| Code | Status | Meaning |
|------|--------|---------|
| 200 | OK | Success ✓ |
| 400 | Bad Request | Invalid input (check error_code) |
| 403 | Forbidden | Need Admin/Operator role |
| 409 | Conflict | UUID collision (rare) |
| 413 | Payload Too Large | File > 1GB |
| 507 | Insufficient Storage | Disk full |
| 500 | Internal Error | Server problem |

---

## Error Codes

| Code | Fix |
|------|-----|
| AOS_INVALID_REQUEST | Missing file field |
| AOS_INVALID_EXTENSION | Rename to .aos |
| AOS_INVALID_RANK | Use 1-512 |
| AOS_INVALID_ALPHA | Use 0.0-100.0 |
| AOS_INVALID_ENUM | Use valid tier/category/scope |
| AOS_INVALID_FORMAT | Check .aos structure |
| AOS_FILE_TOO_LARGE | Reduce file size |
| AOS_PERMISSION_DENIED | Use Admin/Operator token |
| AOS_DB_CONSTRAINT | Retry (UUID collision) |
| AOS_DISK_FULL | Wait for server cleanup |

---

## Response Structure

```json
{
  "adapter_id": "adapter_550e8400e29b41d4a716446655440000",
  "tenant_id": "tenant-1",
  "hash_b3": "blake3hash...",
  "file_path": "./adapters/adapter_550e8400e29b41d4a716446655440000.aos",
  "file_size": 524288000,
  "lifecycle_state": "draft",
  "created_at": "2025-01-19T12:34:56.789Z"
}
```

---

## Common Issues & Fixes

### 400 Bad Request

```bash
# ✗ Forgot file field
curl -F "name=Test" http://...

# ✓ Include file
curl -F "file=@adapter.aos" -F "name=Test" http://...
```

### 403 Forbidden

```bash
# ✗ Using Viewer token (read-only)
curl -H "Authorization: Bearer $VIEWER_TOKEN" ...

# ✓ Use Admin/Operator token
curl -H "Authorization: Bearer $ADMIN_TOKEN" ...
```

### 413 Payload Too Large

```bash
# ✗ File too big
ls -h adapter.aos  # Shows 1.5GB

# ✓ Check actual size and split if needed
stat -c%s adapter.aos  # bytes
```

### 500 Internal Error

```bash
# Retry (might be transient)
sleep 5
curl -X POST ...

# Check server logs
ssh user@server tail -f /var/log/aos/server.log
```

---

## Rate Limiting

**Limit:** 100 uploads/minute per tenant + 50 burst

**Backoff strategy:**

```python
import time
import random

for attempt in range(5):
    try:
        return upload(...)
    except RateLimitError:
        delay = (2 ** attempt) + random.random()
        time.sleep(delay)
```

---

## Performance Tips

1. **Use persistent connection** for batch uploads
2. **Stream large files** (don't buffer entire file)
3. **Set reasonable timeout** (60s for small, 300s for large)
4. **Check disk space** before uploading
5. **Validate locally** first (verify .aos structure)

---

## Pre-Upload Checklist

- [ ] File exists: `ls adapter.aos`
- [ ] Has .aos extension
- [ ] Under 1GB: `stat -c%s adapter.aos`
- [ ] Valid JWT token: `echo $JWT_TOKEN`
- [ ] Token has Admin/Operator role
- [ ] Server reachable: `curl http://localhost:8080/v1/health`

---

## Response Headers

```
HTTP/1.1 200 OK
Content-Type: application/json
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 42
X-RateLimit-Reset: 1705681000
```

---

## Links

- **Full Guide:** [API_UPLOAD_GUIDE.md](API_UPLOAD_GUIDE.md)
- **Troubleshooting:** [UPLOAD_TROUBLESHOOTING.md](UPLOAD_TROUBLESHOOTING.md)
- **Security:** [AUTHENTICATION.md](AUTHENTICATION.md)
- **CLAUDE.md:** [CLAUDE.md](../CLAUDE.md)

---

**Generated:** 2025-01-19
