# AdapterOS .aos Upload API Guide

## Complete Reference for File Upload Operations

**Last Updated:** 2025-01-19
**Status:** Production Ready (PRD-02)
**Target Audience:** Backend engineers, SDK developers, integration partners

---

## Quick Start

The upload endpoint accepts .aos adapter files via HTTP multipart form data with automatic validation, hashing, and database registration.

```bash
# Basic upload example
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -F "file=@adapter.aos" \
  -F "name=My Adapter" \
  -F "tier=persistent"
```

---

## Table of Contents

1. [Endpoint Reference](#endpoint-reference)
2. [Request Format](#request-format)
3. [Response Format](#response-format)
4. [Examples by Language](#examples-by-language)
5. [Error Handling](#error-handling)
6. [Rate Limiting](#rate-limiting)
7. [Size & Format Limits](#size--format-limits)
8. [Troubleshooting Guide](#troubleshooting-guide)
9. [Security Best Practices](#security-best-practices)
10. [Performance Optimization](#performance-optimization)
11. [Migration Guide](#migration-guide)

---

## Endpoint Reference

### POST /v1/adapters/upload-aos

**Purpose:** Upload and register a new .aos adapter file

**Authentication:** Required (Bearer token with `AdapterRegister` permission)

**Method:** POST

**Content-Type:** multipart/form-data

**Rate Limit:** 100 uploads per tenant per minute (token bucket with 50 burst capacity)

### Response Codes

| Code | Status | Meaning |
|------|--------|---------|
| 200 | OK | Upload successful, adapter registered |
| 400 | Bad Request | Invalid input (format, validation, missing fields) |
| 403 | Forbidden | Insufficient permissions (requires Admin/Operator role) |
| 409 | Conflict | Adapter ID collision or duplicate constraint violation |
| 413 | Payload Too Large | File exceeds 1GB maximum size |
| 507 | Insufficient Storage | Disk space exhausted |
| 500 | Internal Error | Server-side failures (database, I/O, corruption) |

---

## Request Format

### Form Fields

All fields are multipart form data:

```
POST /v1/adapters/upload-aos HTTP/1.1
Content-Type: multipart/form-data; boundary=----WebKitFormBoundary

------WebKitFormBoundary
Content-Disposition: form-data; name="file"; filename="adapter.aos"
Content-Type: application/octet-stream

[binary .aos file content]
------WebKitFormBoundary
Content-Disposition: form-data; name="name"

My Adapter Name
------WebKitFormBoundary
Content-Disposition: form-data; name="tier"

persistent
------WebKitFormBoundary--
```

### Field Specifications

#### Required Fields

| Field | Type | Max Length | Description |
|-------|------|------------|-------------|
| `file` | binary | 1GB | .aos archive file (binary) |

#### Optional Fields

| Field | Type | Default | Valid Values | Description |
|-------|------|---------|--------------|-------------|
| `name` | string | filename | 1-256 chars | Display name for adapter |
| `description` | string | empty | any | Human-readable description |
| `tier` | string | ephemeral | ephemeral, warm, persistent | Lifecycle tier |
| `category` | string | general | general, code, text, vision, audio | Adapter purpose category |
| `scope` | string | general | general, public, private, tenant | Access scope |
| `rank` | int | 1 | 1-512 | LoRA rank dimension |
| `alpha` | float | 1.0 | 0.0-100.0 | LoRA scaling factor |

### Field Validation Rules

#### Name Field
- **Length:** 1-256 characters
- **Allowed:** Any printable characters
- **If omitted:** Uses uploaded filename (without .aos extension)

#### Tier Field
- `ephemeral`: Temporary, subject to eviction (default)
- `warm`: Medium-priority, evicted last
- `persistent`: Pinned to memory, survives eviction

#### Category Field
- `general`: Default multi-purpose adapter
- `code`: Code-specific (programming languages)
- `text`: General text processing
- `vision`: Image/video processing
- `audio`: Audio processing

#### Scope Field
- `general`: Default access scope
- `public`: Publicly accessible
- `private`: User/tenant private only
- `tenant`: Tenant-wide accessible

#### Rank Field
- **Range:** 1-512
- **Typical Values:** 4, 8, 16, 32, 64
- **Effect:** Higher rank = more parameters, more memory

#### Alpha Field
- **Range:** 0.0-100.0
- **Typical Values:** 1.0-32.0
- **Effect:** LoRA scaling multiplier (higher = stronger effect)

---

## Response Format

### Success Response (200 OK)

```json
{
  "adapter_id": "adapter_550e8400e29b41d4a716446655440000",
  "tenant_id": "tenant-1",
  "hash_b3": "blake3_hash_hex_string_64_chars",
  "file_path": "./adapters/adapter_550e8400e29b41d4a716446655440000.aos",
  "file_size": 524288000,
  "lifecycle_state": "draft",
  "created_at": "2025-01-19T12:34:56.789Z"
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| adapter_id | string | Unique adapter identifier (UUIDv7-based) |
| tenant_id | string | Your tenant ID from JWT claims |
| hash_b3 | string | BLAKE3 hash of file (64 hex characters) |
| file_path | string | Local storage path on server |
| file_size | integer | File size in bytes |
| lifecycle_state | string | Current state (draft, cold, warm, hot, resident) |
| created_at | string | ISO 8601 timestamp of creation |

### Error Response (4xx/5xx)

```json
{
  "error_code": "AOS_FILE_TOO_LARGE",
  "message": "File exceeds maximum size of 1024MB (received: 1500MB)",
  "details": null
}
```

**Error Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| error_code | string | Programmatic error identifier |
| message | string | User-friendly error message |
| details | string/null | Additional context (if applicable) |

---

## Examples by Language

### cURL

#### Basic Upload

```bash
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -F "file=@my-adapter.aos" \
  -F "name=My Adapter" \
  -F "tier=persistent"
```

#### Complete Upload with All Fields

```bash
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -F "file=@adapter.aos" \
  -F "name=Code Review Adapter" \
  -F "description=Specialized adapter for code review tasks" \
  -F "tier=persistent" \
  -F "category=code" \
  -F "scope=private" \
  -F "rank=16" \
  -F "alpha=8.0" \
  --progress-bar
```

#### Upload with Error Handling

```bash
#!/bin/bash

FILE="adapter.aos"
TOKEN="$1"
BASE_URL="http://localhost:8080"

if [ ! -f "$FILE" ]; then
  echo "Error: File not found: $FILE"
  exit 1
fi

FILE_SIZE=$(stat -f%z "$FILE" 2>/dev/null || stat -c%s "$FILE")
MAX_SIZE=$((1024 * 1024 * 1024))  # 1GB

if [ "$FILE_SIZE" -gt "$MAX_SIZE" ]; then
  echo "Error: File too large ($FILE_SIZE bytes > $MAX_SIZE bytes)"
  exit 1
fi

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/v1/adapters/upload-aos" \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@$FILE" \
  -F "name=$(basename "$FILE" .aos)")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-1)

if [ "$HTTP_CODE" = "200" ]; then
  ADAPTER_ID=$(echo "$BODY" | grep -o '"adapter_id":"[^"]*"' | cut -d'"' -f4)
  echo "Upload successful! Adapter ID: $ADAPTER_ID"
  exit 0
else
  ERROR=$(echo "$BODY" | grep -o '"error_code":"[^"]*"' | cut -d'"' -f4)
  MESSAGE=$(echo "$BODY" | grep -o '"message":"[^"]*"' | cut -d'"' -f4)
  echo "Upload failed ($HTTP_CODE): $ERROR - $MESSAGE"
  exit 1
fi
```

### Python

#### Basic Upload

```python
import requests
import json

def upload_adapter(token: str, file_path: str) -> dict:
    """Upload .aos adapter to AdapterOS"""

    with open(file_path, 'rb') as f:
        files = {
            'file': (f.name, f, 'application/octet-stream'),
            'name': (None, 'My Adapter'),
            'tier': (None, 'persistent'),
        }

        response = requests.post(
            'http://localhost:8080/v1/adapters/upload-aos',
            headers={'Authorization': f'Bearer {token}'},
            files=files,
        )

    response.raise_for_status()
    return response.json()

# Usage
token = "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9..."
result = upload_adapter(token, 'adapter.aos')
print(f"Adapter ID: {result['adapter_id']}")
print(f"File hash: {result['hash_b3']}")
```

#### Production-Grade Upload with Retry

```python
import requests
import time
from pathlib import Path
from dataclasses import dataclass
from typing import Optional

@dataclass
class UploadResult:
    adapter_id: str
    hash_b3: str
    file_path: str
    file_size: int
    lifecycle_state: str
    created_at: str

class AdapterUploader:
    def __init__(self, base_url: str, token: str, max_retries: int = 3):
        self.base_url = base_url
        self.token = token
        self.max_retries = max_retries
        self.session = requests.Session()
        self.session.headers.update({
            'Authorization': f'Bearer {token}'
        })

    def upload(
        self,
        file_path: str,
        name: str,
        tier: str = "ephemeral",
        category: str = "general",
        scope: str = "general",
        rank: int = 1,
        alpha: float = 1.0,
        description: Optional[str] = None
    ) -> UploadResult:
        """
        Upload adapter with automatic retry and validation

        Args:
            file_path: Path to .aos file
            name: Display name for adapter
            tier: Lifecycle tier (ephemeral, warm, persistent)
            category: Adapter category (general, code, text, vision, audio)
            scope: Access scope (general, public, private, tenant)
            rank: LoRA rank (1-512)
            alpha: LoRA scaling factor (0.0-100.0)
            description: Optional description

        Returns:
            UploadResult with adapter metadata

        Raises:
            FileNotFoundError: If file doesn't exist
            ValueError: If validation fails
            requests.HTTPError: If upload fails permanently
        """
        file_path = Path(file_path)
        if not file_path.exists():
            raise FileNotFoundError(f"File not found: {file_path}")

        if not file_path.name.endswith('.aos'):
            raise ValueError(f"File must have .aos extension, got: {file_path.name}")

        file_size = file_path.stat().st_size
        max_size = 1024 * 1024 * 1024  # 1GB
        if file_size > max_size:
            raise ValueError(
                f"File too large: {file_size / (1024*1024):.1f}MB "
                f"(max: {max_size / (1024*1024):.0f}MB)"
            )

        # Validate parameters
        if not 1 <= rank <= 512:
            raise ValueError(f"Rank must be 1-512, got {rank}")
        if not 0.0 <= alpha <= 100.0:
            raise ValueError(f"Alpha must be 0.0-100.0, got {alpha}")

        for attempt in range(1, self.max_retries + 1):
            try:
                return self._do_upload(
                    file_path, name, tier, category, scope, rank, alpha, description
                )
            except requests.exceptions.Timeout:
                if attempt == self.max_retries:
                    raise
                print(f"Timeout on attempt {attempt}, retrying in 2s...")
                time.sleep(2)
            except requests.exceptions.ConnectionError:
                if attempt == self.max_retries:
                    raise
                print(f"Connection error on attempt {attempt}, retrying in 3s...")
                time.sleep(3)

    def _do_upload(
        self,
        file_path: Path,
        name: str,
        tier: str,
        category: str,
        scope: str,
        rank: int,
        alpha: float,
        description: Optional[str]
    ) -> UploadResult:
        """Execute the actual upload request"""
        with open(file_path, 'rb') as f:
            files = {
                'file': (file_path.name, f, 'application/octet-stream'),
                'name': (None, name),
                'tier': (None, tier),
                'category': (None, category),
                'scope': (None, scope),
                'rank': (None, str(rank)),
                'alpha': (None, str(alpha)),
            }

            if description:
                files['description'] = (None, description)

            response = self.session.post(
                f'{self.base_url}/v1/adapters/upload-aos',
                files=files,
                timeout=60.0,
            )

        # Check for errors
        if response.status_code == 400:
            error = response.json()
            raise ValueError(f"Validation error: {error['message']}")
        elif response.status_code == 403:
            raise PermissionError("Insufficient permissions to upload adapters")
        elif response.status_code == 409:
            raise ValueError("Adapter ID conflict (likely UUID collision)")
        elif response.status_code == 413:
            raise ValueError("File too large for endpoint")
        elif response.status_code == 507:
            raise RuntimeError("Server disk space exhausted")

        response.raise_for_status()

        data = response.json()
        return UploadResult(
            adapter_id=data['adapter_id'],
            hash_b3=data['hash_b3'],
            file_path=data['file_path'],
            file_size=data['file_size'],
            lifecycle_state=data['lifecycle_state'],
            created_at=data['created_at'],
        )

# Usage
uploader = AdapterUploader('http://localhost:8080', token)
result = uploader.upload(
    'adapter.aos',
    name='My Code Adapter',
    category='code',
    tier='persistent',
    rank=16,
    alpha=8.0,
    description='For code review tasks'
)
print(f"Uploaded: {result.adapter_id}")
```

### Node.js / TypeScript

#### Basic Upload with Fetch

```typescript
import fs from 'fs';
import path from 'path';

interface UploadResponse {
  adapter_id: string;
  tenant_id: string;
  hash_b3: string;
  file_path: string;
  file_size: number;
  lifecycle_state: string;
  created_at: string;
}

async function uploadAdapter(
  token: string,
  filePath: string,
  name: string
): Promise<UploadResponse> {
  const form = new FormData();
  form.append('file', fs.createReadStream(filePath), path.basename(filePath));
  form.append('name', name);
  form.append('tier', 'persistent');

  const response = await fetch(
    'http://localhost:8080/v1/adapters/upload-aos',
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${token}`,
      },
      body: form,
    }
  );

  if (!response.ok) {
    throw new Error(`Upload failed: ${response.statusText}`);
  }

  return response.json();
}

// Usage
const token = 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9...';
const result = await uploadAdapter(token, './adapter.aos', 'My Adapter');
console.log(`Adapter ID: ${result.adapter_id}`);
```

#### Production SDK with Retry and Progress

```typescript
import FormData from 'form-data';
import fs from 'fs';
import { Readable } from 'stream';

interface UploadOptions {
  name: string;
  description?: string;
  tier?: 'ephemeral' | 'warm' | 'persistent';
  category?: 'general' | 'code' | 'text' | 'vision' | 'audio';
  scope?: 'general' | 'public' | 'private' | 'tenant';
  rank?: number;
  alpha?: number;
}

class AdapterOSClient {
  constructor(
    private baseUrl: string,
    private token: string,
    private maxRetries: number = 3
  ) {}

  async uploadAdapter(
    filePath: string,
    options: UploadOptions,
    onProgress?: (bytes: number, total: number) => void
  ): Promise<UploadResponse> {
    const stats = fs.statSync(filePath);
    const fileSize = stats.size;
    const maxSize = 1024 * 1024 * 1024; // 1GB

    if (fileSize > maxSize) {
      throw new Error(
        `File too large: ${(fileSize / (1024 * 1024)).toFixed(1)}MB ` +
        `(max: ${maxSize / (1024 * 1024)}MB)`
      );
    }

    if (!filePath.endsWith('.aos')) {
      throw new Error(`File must have .aos extension, got: ${filePath}`);
    }

    // Parameter validation
    if (options.rank && (options.rank < 1 || options.rank > 512)) {
      throw new Error(`Rank must be 1-512, got ${options.rank}`);
    }

    if (options.alpha && (options.alpha < 0 || options.alpha > 100)) {
      throw new Error(`Alpha must be 0-100, got ${options.alpha}`);
    }

    for (let attempt = 1; attempt <= this.maxRetries; attempt++) {
      try {
        return await this.performUpload(filePath, options, onProgress);
      } catch (error) {
        if (attempt === this.maxRetries) throw error;

        const delay = attempt === 1 ? 2000 : 5000;
        console.log(`Attempt ${attempt} failed, retrying in ${delay}ms...`);
        await new Promise(resolve => setTimeout(resolve, delay));
      }
    }

    throw new Error('Upload failed after max retries');
  }

  private async performUpload(
    filePath: string,
    options: UploadOptions,
    onProgress?: (bytes: number, total: number) => void
  ): Promise<UploadResponse> {
    const form = new FormData();
    const fileStream = fs.createReadStream(filePath);
    const stats = fs.statSync(filePath);

    form.append('file', fileStream, { filename: filePath });
    form.append('name', options.name);
    form.append('tier', options.tier || 'ephemeral');
    form.append('category', options.category || 'general');
    form.append('scope', options.scope || 'general');
    form.append('rank', String(options.rank || 1));
    form.append('alpha', String(options.alpha || 1.0));

    if (options.description) {
      form.append('description', options.description);
    }

    // Track progress
    let uploadedBytes = 0;
    fileStream.on('data', (chunk) => {
      uploadedBytes += chunk.length;
      onProgress?.(uploadedBytes, stats.size);
    });

    const response = await fetch(`${this.baseUrl}/v1/adapters/upload-aos`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.token}`,
        ...form.getHeaders(),
      },
      body: form,
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(
        `Upload failed (${response.status}): ${error.error_code} - ${error.message}`
      );
    }

    return response.json();
  }
}

// Usage
const client = new AdapterOSClient('http://localhost:8080', token);
const result = await client.uploadAdapter(
  'adapter.aos',
  {
    name: 'Code Review Adapter',
    description: 'Specialized for code review tasks',
    category: 'code',
    tier: 'persistent',
    rank: 16,
    alpha: 8.0,
  },
  (bytes, total) => {
    const percent = ((bytes / total) * 100).toFixed(1);
    console.log(`Upload progress: ${percent}%`);
  }
);

console.log(`Uploaded: ${result.adapter_id}`);
```

### Go

#### Basic Upload

```go
package main

import (
    "bytes"
    "fmt"
    "io"
    "mime/multipart"
    "net/http"
    "os"
    "path/filepath"
)

type UploadResponse struct {
    AdapterID      string `json:"adapter_id"`
    TenantID       string `json:"tenant_id"`
    HashB3         string `json:"hash_b3"`
    FilePath       string `json:"file_path"`
    FileSize       int64  `json:"file_size"`
    LifecycleState string `json:"lifecycle_state"`
    CreatedAt      string `json:"created_at"`
}

func uploadAdapter(baseURL, token, filePath, name string) (*UploadResponse, error) {
    file, err := os.Open(filePath)
    if err != nil {
        return nil, fmt.Errorf("failed to open file: %w", err)
    }
    defer file.Close()

    body := &bytes.Buffer{}
    writer := multipart.NewWriter(body)

    // Add file field
    filePart, err := writer.CreateFormFile("file", filepath.Base(filePath))
    if err != nil {
        return nil, fmt.Errorf("failed to create form file: %w", err)
    }

    if _, err := io.Copy(filePart, file); err != nil {
        return nil, fmt.Errorf("failed to copy file: %w", err)
    }

    // Add name field
    if err := writer.WriteField("name", name); err != nil {
        return nil, fmt.Errorf("failed to write name field: %w", err)
    }

    if err := writer.WriteField("tier", "persistent"); err != nil {
        return nil, fmt.Errorf("failed to write tier field: %w", err)
    }

    if err := writer.Close(); err != nil {
        return nil, fmt.Errorf("failed to close writer: %w", err)
    }

    req, err := http.NewRequest("POST", baseURL+"/v1/adapters/upload-aos", body)
    if err != nil {
        return nil, fmt.Errorf("failed to create request: %w", err)
    }

    req.Header.Set("Authorization", "Bearer "+token)
    req.Header.Set("Content-Type", writer.FormDataContentType())

    client := &http.Client{}
    resp, err := client.Do(req)
    if err != nil {
        return nil, fmt.Errorf("failed to upload: %w", err)
    }
    defer resp.Body.Close()

    if resp.StatusCode != http.StatusOK {
        bodyBytes, _ := io.ReadAll(resp.Body)
        return nil, fmt.Errorf("upload failed with status %d: %s", resp.StatusCode, string(bodyBytes))
    }

    var result UploadResponse
    if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
        return nil, fmt.Errorf("failed to decode response: %w", err)
    }

    return &result, nil
}
```

---

## Error Handling

### Error Codes Reference

| Code | HTTP | Description | Retryable | Action |
|------|------|-------------|-----------|--------|
| AOS_FILE_TOO_LARGE | 413 | File exceeds 1GB | No | Reduce file size or split uploads |
| AOS_DISK_FULL | 507 | Server disk exhausted | Yes | Retry after server cleanup |
| AOS_PERMISSION_DENIED | 403 | Insufficient permissions | No | Use Admin/Operator token |
| AOS_INVALID_FORMAT | 400 | Bad .aos structure | No | Verify file format |
| AOS_INVALID_EXTENSION | 400 | Wrong file extension | No | Rename to .aos |
| AOS_HASH_MISMATCH | 500 | Corruption detected | Yes | Retry (rare) |
| AOS_INVALID_NAME | 400 | Name validation failed | No | Use 1-256 char name |
| AOS_INVALID_REQUEST | 400 | Malformed request | No | Check multipart format |
| AOS_DB_CONSTRAINT | 409 | Duplicate adapter ID | No | Try again (UUID collision) |
| AOS_DB_CONNECTION | 500 | Database error | Yes | Retry after delay |
| AOS_DB_OPERATION | 500 | Generic DB error | No | Check server logs |
| AOS_TEMP_FILE_FAILED | 500 | Temp file error | Yes | Retry |
| AOS_INVALID_PATH | 400 | Path traversal attempt | No | Use simple filename |
| AOS_ID_GEN_FAILED | 500 | UUID generation failed | No | Extremely rare, contact support |
| AOS_INVALID_RANK | 400 | Rank out of bounds | No | Use 1-512 |
| AOS_INVALID_ALPHA | 400 | Alpha out of bounds | No | Use 0.0-100.0 |
| AOS_INVALID_ENUM | 400 | Invalid enum value | No | Use valid tier/category/scope |

### Handling Specific Errors

#### File Too Large (413)

```python
except requests.HTTPError as e:
    if e.response.status_code == 413:
        error = e.response.json()
        max_mb = error['message'].split('of ')[1].split('MB')[0]
        actual_mb = error['message'].split('(received: ')[1].split('MB')[0]
        print(f"File too large: {actual_mb}MB / {max_mb}MB")
        # Split file or compress
```

#### Rate Limit (429)

```python
import time

def upload_with_rate_limit_handling(uploader, file_path, name):
    max_retries = 5
    base_delay = 1

    for attempt in range(max_retries):
        try:
            return uploader.upload(file_path, name)
        except requests.HTTPError as e:
            if e.response.status_code == 429:
                delay = base_delay * (2 ** attempt)  # Exponential backoff
                print(f"Rate limited, waiting {delay}s...")
                time.sleep(delay)
            else:
                raise
```

#### Disk Full (507)

```typescript
async function uploadWithDiskCheck(
  client: AdapterOSClient,
  filePath: string,
  options: UploadOptions
): Promise<UploadResponse> {
  try {
    return await client.uploadAdapter(filePath, options);
  } catch (error) {
    if (error instanceof HttpError && error.status === 507) {
      // Alert operations team
      console.error('Server disk full - alerting ops team');
      await notifyOpsTeam('AdapterOS disk space critical');
      throw new Error('Upload failed: server disk full');
    }
    throw error;
  }
}
```

---

## Rate Limiting

### Rate Limit Behavior

- **Limit:** 100 uploads per tenant per 60 seconds
- **Burst:** 50 additional uploads (token bucket algorithm)
- **Total Capacity:** 150 uploads (100 sustained + 50 burst)
- **Reset:** Automatic every 60 seconds

### Rate Limit Headers

All responses include rate limit information:

```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 42
X-RateLimit-Reset: 1705681000
```

### Handling Rate Limits

```python
import requests
import time

def upload_with_rate_limit_retry(token, file_path, name):
    url = 'http://localhost:8080/v1/adapters/upload-aos'

    while True:
        try:
            response = requests.post(
                url,
                headers={'Authorization': f'Bearer {token}'},
                files={'file': open(file_path, 'rb'), 'name': (None, name)}
            )

            if response.status_code == 429:
                # Extract reset time from header
                reset_at = int(response.headers.get('X-RateLimit-Reset', 0))
                wait_time = max(1, reset_at - int(time.time()))
                print(f"Rate limited. Waiting {wait_time} seconds...")
                time.sleep(wait_time + 1)
                continue

            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"Request failed: {e}")
            raise
```

---

## Size & Format Limits

### File Size Limits

| Type | Limit | Notes |
|------|-------|-------|
| Max file size | 1 GB | Practical limit (memory constraints) |
| Min file size | 8 bytes | Must have valid header |
| Typical size | 50-500 MB | Most adapter files |

### .aos File Format

The .aos format is a binary archive with this structure:

```
[0-3]   manifest_offset (u32 LE)
[4-7]   manifest_len (u32 LE)
[offset] manifest (JSON)
[offset] weights (safetensors binary)
```

#### Manifest Specification

```json
{
  "version": "1.0.0",
  "name": "string (optional)",
  "description": "string (optional)",
  "model_type": "lora",
  "base_model": "string",
  "rank": 16,
  "alpha": 8.0,
  "adapter_type": "lora"
}
```

#### Validation Rules

1. **Header:** Exactly 8 bytes (two u32 LE values)
2. **Manifest offset:** Must be >= 8
3. **Manifest length:** Must be > 0
4. **Manifest content:** Valid JSON object (not array or primitive)
5. **Weights:** Valid safetensors format (if present)
6. **File bounds:** Manifest must fit within file size

#### Example: Creating Valid .aos File

```python
import json
import struct
from pathlib import Path

def create_aos_file(output_path: str, manifest: dict, weights: bytes = b'{}'):
    """Create valid .aos file"""

    # Validate manifest
    manifest_json = json.dumps(manifest).encode('utf-8')
    if len(manifest_json) == 0:
        raise ValueError("Manifest cannot be empty")

    # Validate weights are valid safetensors
    if len(weights) < 8:
        raise ValueError("Weights must be at least 8 bytes")

    # Build .aos file
    manifest_offset = 8  # After 8-byte header
    manifest_len = len(manifest_json)

    with open(output_path, 'wb') as f:
        # Write header
        f.write(struct.pack('<I', manifest_offset))
        f.write(struct.pack('<I', manifest_len))

        # Write manifest
        f.write(manifest_json)

        # Write weights
        f.write(weights)

# Usage
manifest = {
    "version": "1.0.0",
    "model_type": "lora",
    "base_model": "llama",
    "rank": 16,
    "alpha": 8.0,
}

create_aos_file('adapter.aos', manifest)
```

---

## Troubleshooting Guide

### Upload Fails with 400 Bad Request

**Symptom:** Consistently getting 400 with "Invalid request format"

**Possible Causes:**
1. Missing or empty `file` field
2. File is not actually binary (CRLF line endings from text mode)
3. Multipart boundary incorrect

**Solutions:**

```python
# Ensure file is opened in binary mode
with open('adapter.aos', 'rb') as f:  # ← 'rb' is critical
    files = {'file': f}
    response = requests.post(url, files=files)

# Verify multipart structure
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@adapter.aos" \
  -v  # ← verbose to see exact request
```

### Upload Fails with 413 Payload Too Large

**Symptom:** File upload rejected even though file < 1GB

**Possible Causes:**
1. Proxy with smaller limit (nginx, HAProxy, etc.)
2. Web server misconfiguration
3. Actual file is larger than reported

**Solutions:**

```bash
# Check actual file size
ls -lh adapter.aos  # Human readable
stat -f%z adapter.aos  # Bytes (macOS)
stat -c%s adapter.aos  # Bytes (Linux)

# Check server limits
curl -I http://localhost:8080/v1/adapters/upload-aos  # See response headers
```

### Upload Fails with 403 Forbidden

**Symptom:** "Upload failed: insufficient permissions"

**Possible Causes:**
1. Using Viewer or Compliance role (read-only)
2. JWT token expired
3. Token missing AdapterRegister permission

**Solutions:**

```bash
# Verify token has correct role
jwt_decode() {
  jq -R 'split(".") | .[1] | @base64d | fromjson' <<< "$1"
}

jwt_decode "$JWT_TOKEN"
# Look for "role": "Admin" or "role": "Operator"

# Get new token if expired
# Contact admin to grant AdapterRegister permission if needed
```

### Upload Succeeds but Adapter Not Usable

**Symptom:** Upload returns 200, but adapter_id not found in listings

**Possible Causes:**
1. Adapter in "draft" state (not promoted)
2. Different tenant context
3. Database race condition

**Solutions:**

```bash
# Check adapter status
curl -X GET http://localhost:8080/v1/adapters/$ADAPTER_ID \
  -H "Authorization: Bearer $TOKEN"

# Check if tenant matches your claims
# Verify token tenant_id matches request context
```

### Hash Mismatch Error (500)

**Symptom:** "File corruption detected: hash mismatch"

**Possible Causes:**
1. Network corruption during upload
2. File system issue on server
3. Extremely rare: bad memory

**Solutions:**

```python
# Verify local file integrity
import hashlib

def verify_file_integrity(file_path):
    """Compute BLAKE3 hash locally"""
    import blake3
    h = blake3.blake3()

    with open(file_path, 'rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            h.update(chunk)

    return h.hexdigest()

local_hash = verify_file_integrity('adapter.aos')
print(f"Local hash: {local_hash}")

# Retry upload
response = upload_adapter(token, 'adapter.aos', 'My Adapter')
if response['hash_b3'] != local_hash:
    print("WARNING: Server hash doesn't match local!")
    # Investigate network or server issues
```

### Rate Limit Exceeded (429)

**Symptom:** Rapid sequential uploads fail with 429

**Possible Causes:**
1. Uploading faster than rate limit allows
2. Burst capacity exhausted
3. Multiple clients using same tenant token

**Solutions:**

```python
# Implement exponential backoff
import time
import random

def upload_with_backoff(uploader, file_path, name):
    max_attempts = 5
    base_delay = 1

    for attempt in range(max_attempts):
        try:
            return uploader.upload(file_path, name)
        except RateLimitError:
            if attempt >= max_attempts - 1:
                raise

            # Exponential backoff with jitter
            delay = base_delay * (2 ** attempt)
            jitter = random.uniform(0, delay * 0.1)
            total_delay = delay + jitter

            print(f"Rate limited on attempt {attempt + 1}, " \
                  f"waiting {total_delay:.1f}s...")
            time.sleep(total_delay)

# For bulk uploads, use queue with controlled rate
from queue import Queue
from threading import Thread

def bulk_upload_with_rate_control(uploader, files, rate_per_second=1):
    """Upload multiple files at controlled rate"""
    delay = 1.0 / rate_per_second

    for file_path, name in files:
        uploader.upload(file_path, name)
        time.sleep(delay)
```

### Database Constraint Violation (409)

**Symptom:** Upload succeeds but database registration fails

**Possible Causes:**
1. UUID collision (extremely rare)
2. Adapter ID already exists
3. Database constraint conflict

**Solutions:**

```bash
# Check if adapter already exists
curl -X GET http://localhost:8080/v1/adapters?query="$NAME" \
  -H "Authorization: Bearer $TOKEN"

# If it exists, delete and retry
curl -X DELETE http://localhost:8080/v1/adapters/$ADAPTER_ID \
  -H "Authorization: Bearer $TOKEN"
```

---

## Security Best Practices

### Authentication

1. **Always use HTTPS** in production
   ```python
   # ✗ Don't do this
   response = requests.post('http://localhost:8080/...')

   # ✓ Do this
   response = requests.post('https://api.adapters.local/...')
   ```

2. **Validate JWT tokens** before sending
   ```python
   import jwt

   def validate_token(token: str, secret: str) -> bool:
       try:
           jwt.decode(token, secret, algorithms=['EdDSA'])
           return True
       except jwt.InvalidSignatureError:
           return False
   ```

3. **Use short-lived tokens** (8 hour TTL)
   ```python
   # Refresh token before expiry
   def upload_with_token_refresh(client, file_path, name):
       token = client.get_current_token()

       # Check if token expires in next 5 minutes
       if token_expires_soon(token, threshold=300):
           token = client.refresh_token()

       return client.upload(file_path, name, token)
   ```

### File Validation

1. **Verify file size before upload**
   ```python
   import os

   max_size = 1024 * 1024 * 1024  # 1GB
   file_size = os.path.getsize('adapter.aos')

   if file_size > max_size:
       raise ValueError(f"File too large: {file_size / (1024*1024):.0f}MB")
   ```

2. **Check file extension**
   ```python
   if not file_path.endswith('.aos'):
       raise ValueError("File must have .aos extension")
   ```

3. **Validate manifest before sending**
   ```python
   import struct
   import json

   def validate_aos_structure(file_path: str) -> bool:
       with open(file_path, 'rb') as f:
           # Read header
           header = f.read(8)
           if len(header) < 8:
               return False

           offset, length = struct.unpack('<II', header)

           # Validate bounds
           if offset < 8 or length == 0:
               return False

           # Read and parse manifest
           f.seek(offset)
           manifest_bytes = f.read(length)

           try:
               manifest = json.loads(manifest_bytes)
               return isinstance(manifest, dict)
           except json.JSONDecodeError:
               return False
   ```

### Request Security

1. **Use secure random for client-side IDs**
   ```python
   import secrets
   request_id = secrets.token_hex(16)
   # Not: uuid.uuid4() or random.random()
   ```

2. **Don't log sensitive data**
   ```python
   # ✗ Don't do this
   print(f"Uploading with token: {token}")

   # ✓ Do this
   print(f"Uploading adapter: {adapter_name}")
   logger.debug(f"Request ID: {request_id}")
   ```

3. **Validate responses** before using
   ```python
   response = requests.post(...)

   # Verify required fields
   required = {'adapter_id', 'hash_b3', 'file_size'}
   if not required.issubset(response.json().keys()):
       raise ValueError("Response missing required fields")
   ```

### Network Security

1. **Use persistent connections** to reduce overhead
   ```python
   from requests.adapters import HTTPAdapter
   from urllib3.util.retry import Retry

   session = requests.Session()
   retries = Retry(
       total=3,
       backoff_factor=1.0,
       status_forcelist=[500, 502, 503, 504]
   )
   adapter = HTTPAdapter(max_retries=retries)
   session.mount('https://', adapter)
   ```

2. **Set reasonable timeouts**
   ```python
   # Never use infinite timeouts
   response = requests.post(url, timeout=60)  # 60 seconds

   # For large files, use longer timeout
   response = requests.post(url, timeout=300)  # 5 minutes
   ```

3. **Verify server certificate** (HTTPS only)
   ```python
   # ✓ Verify certificate (default in requests)
   response = requests.post(url, verify=True)

   # ✗ Never disable verification in production
   response = requests.post(url, verify=False)  # DON'T DO THIS
   ```

---

## Performance Optimization

### Batch Uploads

For uploading multiple adapters:

```python
from concurrent.futures import ThreadPoolExecutor
import time

def batch_upload(token, files: list, max_workers=3):
    """Upload multiple files in parallel with rate limiting"""

    def upload_file(file_info):
        file_path, name, tier = file_info
        uploader = AdapterUploader('http://localhost:8080', token)
        return uploader.upload(file_path, name, tier=tier)

    results = []
    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        for result in executor.map(upload_file, files):
            results.append(result)
            time.sleep(0.5)  # Rate limit: ~2 uploads/second

    return results
```

### Large File Optimization

For files > 100MB:

```python
def upload_large_file(token, file_path, name):
    """Optimized for large files"""

    uploader = AdapterUploader('http://localhost:8080', token)

    # Show progress
    def show_progress(bytes_done, total):
        pct = (bytes_done / total) * 100
        print(f"Progress: {pct:.1f}% ({bytes_done / (1024*1024):.0f}MB)")

    # Use streaming if available
    result = uploader.upload(
        file_path,
        name,
        stream=True,
        on_progress=show_progress
    )

    return result
```

### Connection Pooling

```python
import requests
from requests.adapters import HTTPAdapter
from urllib3.poolmanager import PoolManager

class UploadClient:
    def __init__(self, base_url, token):
        self.session = requests.Session()

        # Configure connection pooling
        adapter = HTTPAdapter(
            pool_connections=10,
            pool_maxsize=20,
            max_retries=3
        )
        self.session.mount('http://', adapter)
        self.session.mount('https://', adapter)

        self.base_url = base_url
        self.token = token

    def upload(self, file_path, name):
        # Reuses connection from pool
        return self.session.post(
            f'{self.base_url}/v1/adapters/upload-aos',
            headers={'Authorization': f'Bearer {self.token}'},
            files={'file': open(file_path, 'rb'), 'name': (None, name)}
        )
```

### Monitoring Upload Performance

```python
import time
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

def upload_with_metrics(uploader, file_path, name):
    """Track upload performance metrics"""

    import os
    file_size = os.path.getsize(file_path)

    start_time = time.time()
    try:
        result = uploader.upload(file_path, name)
        duration = time.time() - start_time
        throughput = file_size / (1024 * 1024) / duration  # MB/s

        logger.info(
            f"Upload successful: "
            f"size={file_size / (1024*1024):.1f}MB, "
            f"time={duration:.1f}s, "
            f"throughput={throughput:.1f}MB/s"
        )

        return result
    except Exception as e:
        duration = time.time() - start_time
        logger.error(f"Upload failed after {duration:.1f}s: {e}")
        raise
```

---

## Migration Guide

### From Previous Upload System (if applicable)

#### Changes in Endpoint

| Aspect | Old | New |
|--------|-----|-----|
| Endpoint | `/api/upload` | `/v1/adapters/upload-aos` |
| Method | POST | POST |
| Content-Type | multipart/form-data | multipart/form-data |
| Auth | API key | Bearer JWT |
| Response | Custom format | Standardized JSON |

#### Migration Steps

1. **Update endpoint URL**
   ```python
   # Old
   response = requests.post('http://localhost:8080/api/upload', ...)

   # New
   response = requests.post('http://localhost:8080/v1/adapters/upload-aos', ...)
   ```

2. **Update authentication**
   ```python
   # Old
   headers = {'X-API-Key': api_key}

   # New
   headers = {'Authorization': f'Bearer {jwt_token}'}
   ```

3. **Verify response structure**
   ```python
   # Response now includes additional fields
   result = response.json()
   adapter_id = result['adapter_id']  # New: UUIDv7-based
   hash_b3 = result['hash_b3']        # New: BLAKE3 hash
   lifecycle_state = result['lifecycle_state']  # New: draft state
   ```

4. **Handle new error codes**
   ```python
   # Old: error field in response
   # New: error_code + message structure

   if response.status_code != 200:
       error = response.json()
       code = error['error_code']
       message = error['message']
   ```

---

## Support & Feedback

- **Issues:** Report bugs via GitHub issues
- **Questions:** Check CLAUDE.md for architecture details
- **Performance:** Profile with included metrics
- **Security:** Report vulnerabilities to security@adapters.local

---

**Document maintained by:** AdapterOS Team
**Last verified:** 2025-01-19
