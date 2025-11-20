# AdapterOS Upload Troubleshooting Guide

## Comprehensive Debugging for Upload Issues

**Last Updated:** 2025-01-19
**Quick Reference Version:** See [API_UPLOAD_GUIDE.md](API_UPLOAD_GUIDE.md)

---

## Diagnostic Checklist

Use this checklist to systematically identify upload issues:

### Pre-Upload Checks

- [ ] File exists and is readable: `ls -lh adapter.aos`
- [ ] File has .aos extension: `file adapter.aos`
- [ ] File size is under 1GB: `stat -c%s adapter.aos`
- [ ] JWT token is valid: `jwt_decode $TOKEN`
- [ ] Token has AdapterRegister permission: `role: Admin/Operator`
- [ ] Network connectivity: `ping api.example.com`
- [ ] HTTPS certificate valid (if applicable): `openssl s_client -connect host:443`

### File Validation Checks

```bash
#!/bin/bash

FILE="adapter.aos"

# Check file exists
test -f "$FILE" || { echo "File not found: $FILE"; exit 1; }

# Check extension
[[ "$FILE" == *.aos ]] || { echo "Wrong extension: $FILE"; exit 1; }

# Check size
SIZE=$(stat -c%s "$FILE" 2>/dev/null || stat -f%z "$FILE")
MAX=$((1024 * 1024 * 1024))
[ "$SIZE" -le "$MAX" ] || { echo "File too large: $SIZE bytes"; exit 1; }

# Check it's actually binary
file "$FILE" | grep -q "data" || echo "Warning: File might not be binary"

# Validate .aos structure (first 8 bytes)
HEADER=$(od -An -tx1 -N8 "$FILE")
echo "File header: $HEADER"

echo "✓ All pre-checks passed"
```

---

## HTTP Status Code Guide

### 200 OK - Success

**Meaning:** Upload completed successfully, adapter registered

**What to do:**
- Adapter is now available with ID in response
- Check lifecycle state is "draft"
- Verify adapter appears in listings

```bash
# Verify adapter exists
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/$ADAPTER_ID

# Check in listings
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters?category=code
```

---

### 400 Bad Request - Client Error

**Possible causes:**
1. Invalid multipart structure
2. Missing required file field
3. Invalid field values
4. Malformed JSON
5. File not actually binary

**Diagnostic steps:**

```bash
# Check exact error message
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@adapter.aos" \
  -F "name=Test" \
  http://localhost:8080/v1/adapters/upload-aos)

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-1)

if [ "$HTTP_CODE" = "400" ]; then
  echo "Error response:"
  echo "$BODY" | jq .
fi
```

**Solutions by error code:**

#### AOS_INVALID_REQUEST

```json
{
  "error_code": "AOS_INVALID_REQUEST",
  "message": "No file provided"
}
```

**Fix:** Ensure file field is included
```bash
# ✗ Wrong: no file field
curl -F "name=Test" http://localhost:8080/v1/adapters/upload-aos

# ✓ Correct: includes file
curl -F "file=@adapter.aos" -F "name=Test" \
  http://localhost:8080/v1/adapters/upload-aos
```

#### AOS_INVALID_EXTENSION

```json
{
  "error_code": "AOS_INVALID_EXTENSION",
  "message": "Invalid file extension: expected .aos, got .txt"
}
```

**Fix:** Rename file to have .aos extension
```bash
mv adapter.model adapter.aos
```

#### AOS_INVALID_RANK

```json
{
  "error_code": "AOS_INVALID_RANK",
  "message": "Invalid rank value: must be between 1 and 512, got 1024"
}
```

**Fix:** Use rank between 1-512
```bash
curl -F "file=@adapter.aos" \
  -F "rank=16" \  # ← Valid value
  http://localhost:8080/v1/adapters/upload-aos
```

#### AOS_INVALID_ALPHA

```json
{
  "error_code": "AOS_INVALID_ALPHA",
  "message": "Invalid alpha value: must be between 0.0 and 100.0, got 150.5"
}
```

**Fix:** Use alpha between 0.0-100.0
```bash
curl -F "file=@adapter.aos" \
  -F "alpha=8.0" \  # ← Valid value
  http://localhost:8080/v1/adapters/upload-aos
```

#### AOS_INVALID_ENUM

```json
{
  "error_code": "AOS_INVALID_ENUM",
  "message": "Invalid tier value 'super_fast': must be one of: ephemeral, warm, persistent"
}
```

**Fix:** Use valid enum value
```bash
curl -F "file=@adapter.aos" \
  -F "tier=persistent" \  # ← Valid value
  http://localhost:8080/v1/adapters/upload-aos
```

#### AOS_INVALID_FORMAT

```json
{
  "error_code": "AOS_INVALID_FORMAT",
  "message": "Invalid .aos file format: manifest is not a JSON object"
}
```

**Fix:** Verify .aos file structure
```python
import struct
import json

def check_aos_format(file_path):
    with open(file_path, 'rb') as f:
        # Read header
        header = f.read(8)
        if len(header) < 8:
            return "File too small"

        offset, length = struct.unpack('<II', header)
        print(f"Manifest offset: {offset}, length: {length}")

        if offset < 8 or length == 0:
            return "Invalid header values"

        # Read manifest
        f.seek(offset)
        manifest_bytes = f.read(length)

        try:
            manifest = json.loads(manifest_bytes)
            if not isinstance(manifest, dict):
                return "Manifest must be JSON object, not array"
            print("✓ Valid .aos structure")
            return None
        except json.JSONDecodeError as e:
            return f"Invalid JSON manifest: {e}"

error = check_aos_format('adapter.aos')
if error:
    print(f"✗ {error}")
```

---

### 403 Forbidden - Permission Error

**Meaning:** JWT token lacks AdapterRegister permission

**Who can upload:**
- Admin role (all permissions)
- Operator role (runtime operations)
- NOT: Viewer or Compliance (read-only)

**Diagnostic steps:**

```bash
# Decode JWT to check role
function jwt_decode() {
  jq -R 'split(".") | .[1] | @base64d | fromjson' <<< "$1"
}

jwt_decode "$JWT_TOKEN"

# Output should show:
# {
#   "role": "Admin",  ← Must be Admin or Operator
#   "tenant_id": "tenant-1",
#   ...
# }
```

**Solutions:**

```bash
# Get token with correct role
TOKEN=$(curl -X POST http://localhost:8080/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "admin@example.com",
    "password": "password"
  }' | jq -r .token)

# Verify new token has Admin role
jwt_decode "$TOKEN" | grep role

# Retry upload with new token
curl -F "file=@adapter.aos" \
  -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/upload-aos
```

---

### 409 Conflict - Database Constraint Error

**Meaning:** Adapter ID already exists (UUID collision or constraint violation)

**Why it happens:**
- Very rare: UUID collision (< 1 in 10 billion)
- More common: Previous upload with same ID not yet garbage collected

**Solutions:**

```bash
# Check if adapter already exists
ADAPTER_ID="adapter_550e8400e29b41d4a716446655440000"

curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/$ADAPTER_ID

# If it exists and is old, delete it first
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/$ADAPTER_ID

# Retry upload (will generate new UUID)
curl -F "file=@adapter.aos" \
  -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/upload-aos
```

---

### 413 Payload Too Large - File Too Big

**Meaning:** File exceeds 1GB size limit

**What to check:**
1. Actual file size vs reported size
2. Compression before upload
3. Proxy limits (nginx, HAProxy)

**Diagnostic steps:**

```bash
# Get actual file size
ls -lh adapter.aos
stat -c%s adapter.aos  # Bytes

# Convert to GB
SIZE_BYTES=$(stat -c%s adapter.aos)
SIZE_GB=$((SIZE_BYTES / 1024 / 1024 / 1024))
echo "File size: $SIZE_GB GB"

if [ "$SIZE_GB" -gt 1 ]; then
  echo "✗ File exceeds 1GB limit"
fi

# Check if file is compressible
gzip -9 -c adapter.aos | wc -c  # Compressed size
```

**Solutions:**

```bash
# Option 1: Compress before upload (if supported by server)
gzip adapter.aos
curl -F "file=@adapter.aos.gz" http://localhost:8080/v1/adapters/upload-aos

# Option 2: Split into multiple adapters
# (Usually indicates adapter might be too large anyway)
split -b 500M adapter.aos part_

# Option 3: Remove unnecessary weights
# Use external tool to clean up .aos file
python3 optimize_aos.py adapter.aos adapter_optimized.aos
```

---

### 507 Insufficient Storage - Disk Full

**Meaning:** Server disk space exhausted

**Impact:** Uploads fail until disk space is freed

**Diagnostic steps:**

```bash
# SSH to server and check disk usage
ssh user@server
df -h /  # Overall disk usage
du -sh /adapters/  # Adapter directory size

# Check if specific filesystem is full
mount | grep adapters
```

**Solutions:**

```bash
# Wait for automatic cleanup (TTL-based adapters deleted)
# Or ask admin to:
# 1. Clean up old adapters
# 2. Extend disk storage
# 3. Enable compression

# Check when it will resolve
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters?status=expired

# Delete old adapters manually if needed
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/$OLD_ADAPTER_ID
```

---

### 500 Internal Server Error - Server Error

**Possible causes:**
1. Database connection error
2. File system error
3. File corruption detected
4. Temporary server issue

**Diagnostic steps:**

```bash
# Check server logs for detailed error
ssh user@server
tail -f /var/log/aos/server.log

# Look for patterns:
# - "Database connection error"
# - "Hash mismatch"
# - "IO error"
# - "Permission denied"

# Check server health
curl http://localhost:8080/v1/health

# Try uploading again (might be transient)
sleep 5
curl -F "file=@adapter.aos" \
  -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/upload-aos
```

#### AOS_HASH_MISMATCH

```json
{
  "error_code": "AOS_HASH_MISMATCH",
  "message": "File integrity check failed: hash mismatch"
}
```

**Cause:** File corruption during write

**Solutions:**

```python
# Verify local file integrity first
import blake3

def verify_file(path):
    h = blake3.blake3()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            h.update(chunk)
    return h.hexdigest()

local_hash = verify_file('adapter.aos')
print(f"Local hash: {local_hash}")

# If local file is fine, network corruption happened
# Retry with fresh connection
import requests
session = requests.Session()  # Fresh session
session.post(...)
```

#### AOS_DB_CONNECTION

```json
{
  "error_code": "AOS_DB_CONNECTION",
  "message": "Database connection failed: timeout after 30s"
}
```

**Cause:** Database unavailable or overloaded

**Solutions:**

```bash
# Check database connectivity
ssh user@server
nc -zv db.example.com 5432  # PostgreSQL
nc -zv localhost 3306  # MySQL

# Check database load
mysql -u root -p -e "SHOW PROCESSLIST;"

# Retry with exponential backoff
for i in 1 2 3; do
  curl -F "file=@adapter.aos" ... && break
  echo "Attempt $i failed, waiting..."
  sleep $((2 ** i))
done
```

#### AOS_TEMP_FILE_FAILED

```json
{
  "error_code": "AOS_TEMP_FILE_FAILED",
  "message": "Temporary file error: cannot create temp file"
}
```

**Cause:** Can't write temporary files (permissions, space, etc.)

**Solutions:**

```bash
# Check /adapters directory permissions
ls -ld /adapters/
# Should be: drwxrwxr-x (or similar, writable by process user)

# Check disk space in temp directory
df -h /tmp
df -h /adapters

# Check process user
ps aux | grep aos-server
# Note the user, then check permissions
sudo ls -l /adapters | head -5

# Fix permissions if needed
sudo chown aos:aos /adapters
sudo chmod 755 /adapters
```

---

## Network-Related Issues

### SSL/TLS Certificate Error

**Symptom:** "certificate verify failed" or "CERTIFICATE_VERIFY_FAILED"

```python
import requests
import urllib3

# Disable SSL verification (DEVELOPMENT ONLY)
urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)
response = requests.post(url, verify=False)  # DON'T USE IN PRODUCTION

# Better: Use self-signed certificate
response = requests.post(
    url,
    verify='/path/to/ca-cert.pem'  # Provide CA cert
)
```

**Proper solution:**

```bash
# Get server certificate
openssl s_client -connect api.example.com:443 -showcerts

# Save to PEM file
openssl s_client -connect api.example.com:443 -showcerts \
  </dev/null 2>/dev/null | \
  openssl x509 -outform PEM > ca-cert.pem

# Use in requests
curl --cacert ca-cert.pem https://api.example.com/...
```

### Connection Timeout

**Symptom:** "Connection timed out" or "Read timed out"

**Causes:**
1. Network unreachable
2. Server not responding
3. Firewall blocking
4. Request too large for timeout
5. Slow network

**Solutions:**

```python
import requests
import socket

# Test basic connectivity
def test_connectivity(host, port):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(5)
    try:
        sock.connect((host, port))
        print(f"✓ Can connect to {host}:{port}")
        return True
    except socket.timeout:
        print(f"✗ Timeout connecting to {host}:{port}")
        return False
    except socket.error as e:
        print(f"✗ Cannot connect: {e}")
        return False
    finally:
        sock.close()

test_connectivity('localhost', 8080)

# Increase timeout for large files
response = requests.post(
    url,
    files=files,
    timeout=300  # 5 minutes for large files
)

# Use adaptive timeout based on file size
import os
file_size = os.path.getsize('adapter.aos')
timeout = max(60, file_size / (1024 * 1024))  # 1 second per MB minimum
response = requests.post(url, files=files, timeout=timeout)
```

### Connection Refused

**Symptom:** "Connection refused" or "Errno 111"

**Meaning:** Server not listening on that port/address

**Solutions:**

```bash
# Verify server is running
curl http://localhost:8080/v1/health

# If not accessible, check if server is listening
lsof -i :8080
netstat -tlnp | grep 8080

# Verify you're connecting to correct address
# Check your config for the API URL
echo $API_URL

# Try with verbose output
curl -v http://localhost:8080/v1/adapters/upload-aos
```

---

## Performance Issues

### Slow Upload Speed

**Symptom:** Upload takes much longer than expected

**Diagnostic steps:**

```bash
# Measure upload speed
time curl -F "file=@adapter.aos" http://localhost:8080/v1/adapters/upload-aos

# Calculate throughput
FILE_SIZE=$(stat -c%s adapter.aos)
FILE_SIZE_MB=$((FILE_SIZE / 1024 / 1024))
TIME_SECONDS=45  # Replace with actual time
THROUGHPUT=$((FILE_SIZE_MB / TIME_SECONDS))
echo "Throughput: $THROUGHPUT MB/s"

# Check network bandwidth
iperf3 -c server.example.com  # Network test
```

**Solutions:**

```python
# Use persistent connection
from requests.adapters import HTTPAdapter

session = requests.Session()
adapter = HTTPAdapter(
    pool_connections=1,
    pool_maxsize=1,
)
session.mount('http://', adapter)

# Optimize chunk size for your network
def upload_with_optimized_chunks(session, url, file_path):
    chunk_size = 1024 * 1024  # 1MB chunks
    with open(file_path, 'rb') as f:
        files = {'file': f}
        return session.post(url, files=files)

# Use multipart streaming (if supported)
def stream_large_file(session, url, file_path):
    with open(file_path, 'rb') as f:
        return session.post(
            url,
            data=f,
            headers={'Content-Type': 'application/octet-stream'}
        )
```

### Memory Usage Spike

**Symptom:** Process uses excessive memory during upload

**Solutions:**

```python
# Don't load entire file into memory
# ✗ Wrong: reads entire file into memory
response = requests.post(url, files={'file': open('big.aos', 'rb').read()})

# ✓ Correct: streams from disk
response = requests.post(url, files={'file': open('big.aos', 'rb')})

# Or use streaming with chunks
def upload_streaming(session, url, file_path):
    with open(file_path, 'rb') as f:
        # Stream in chunks without loading all in memory
        session.post(url, data=iter_file_chunks(f))

def iter_file_chunks(file_obj, chunk_size=1024*1024):
    while True:
        chunk = file_obj.read(chunk_size)
        if not chunk:
            break
        yield chunk
```

---

## Testing & Validation

### Pre-Flight Validation Script

```bash
#!/bin/bash
# validate_upload.sh - Comprehensive pre-upload validation

set -e

FILE="${1:?Usage: $0 <file.aos>}"
TOKEN="${2:?Usage: $0 <file.aos> <JWT_TOKEN>}"
API_URL="${3:-http://localhost:8080}"

echo "Validating upload for: $FILE"
echo "API URL: $API_URL"
echo

# 1. File existence and size
echo "1. Checking file..."
test -f "$FILE" || { echo "✗ File not found: $FILE"; exit 1; }
SIZE=$(stat -c%s "$FILE" 2>/dev/null || stat -f%z "$FILE")
MAX=$((1024 * 1024 * 1024))
if [ "$SIZE" -gt "$MAX" ]; then
  echo "✗ File too large: $(($SIZE / 1024 / 1024)) MB > 1024 MB"
  exit 1
fi
echo "✓ File size OK: $(($SIZE / 1024 / 1024)) MB"

# 2. Extension check
echo
echo "2. Checking file extension..."
[[ "$FILE" == *.aos ]] || { echo "✗ Wrong extension: $FILE"; exit 1; }
echo "✓ Extension is .aos"

# 3. Token validation
echo
echo "3. Validating JWT token..."
function jwt_decode() {
  jq -R 'split(".") | .[1] | @base64d | fromjson' <<< "$1" 2>/dev/null
}
CLAIMS=$(jwt_decode "$TOKEN")
ROLE=$(echo "$CLAIMS" | jq -r .role)
TENANT=$(echo "$CLAIMS" | jq -r .tenant_id)

if [[ ! "$ROLE" =~ ^(Admin|Operator)$ ]]; then
  echo "✗ Wrong role: $ROLE (need Admin or Operator)"
  exit 1
fi
echo "✓ Token role: $ROLE"
echo "✓ Tenant ID: $TENANT"

# 4. Network connectivity
echo
echo "4. Testing network connectivity..."
TIMEOUT_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 5 "$API_URL/v1/health")
if [ "$TIMEOUT_STATUS" = "000" ]; then
  echo "✗ Cannot reach server: $API_URL"
  exit 1
fi
echo "✓ Server reachable (HTTP $TIMEOUT_STATUS)"

# 5. Disk space check
echo
echo "5. Checking server disk space..."
DISK_RESPONSE=$(curl -s -H "Authorization: Bearer $TOKEN" "$API_URL/v1/system/disk")
AVAILABLE=$(echo "$DISK_RESPONSE" | jq -r .available_mb 2>/dev/null || echo "unknown")
if [ "$AVAILABLE" != "unknown" ]; then
  NEEDED=$(($SIZE / 1024 / 1024))
  if [ "$AVAILABLE" -lt "$NEEDED" ]; then
    echo "✗ Insufficient disk space: need $NEEDED MB, have $AVAILABLE MB"
    exit 1
  fi
  echo "✓ Disk space OK: $AVAILABLE MB available"
fi

# 6. .aos file structure validation
echo
echo "6. Validating .aos file structure..."
python3 - <<EOF
import struct
import json
import sys

def validate_aos(path):
    try:
        with open(path, 'rb') as f:
            header = f.read(8)
            if len(header) < 8:
                return False, "File too small for header"

            offset, length = struct.unpack('<II', header)
            if offset < 8 or length == 0:
                return False, f"Invalid header: offset={offset}, length={length}"

            f.seek(offset)
            manifest_bytes = f.read(length)
            manifest = json.loads(manifest_bytes)

            if not isinstance(manifest, dict):
                return False, "Manifest must be JSON object"

            return True, f"Valid .aos (manifest: {len(manifest)} fields)"
    except Exception as e:
        return False, str(e)

valid, msg = validate_aos("$FILE")
if valid:
    print(f"✓ .aos structure: {msg}")
    sys.exit(0)
else:
    print(f"✗ Invalid .aos file: {msg}")
    sys.exit(1)
EOF

# All checks passed
echo
echo "✅ All validation checks passed!"
echo "Ready to upload with:"
echo "  curl -F 'file=@$FILE' -H 'Authorization: Bearer \$TOKEN' $API_URL/v1/adapters/upload-aos"
```

### Unit Test for Upload Handler

```python
import pytest
from pathlib import Path
import json
import struct

def create_minimal_aos(path: Path):
    """Create minimal valid .aos file"""
    manifest = json.dumps({
        "version": "1.0.0",
        "model_type": "lora",
        "base_model": "llama",
        "rank": 16,
        "alpha": 8.0
    }).encode()

    with open(path, 'wb') as f:
        f.write(struct.pack('<II', 8, len(manifest)))
        f.write(manifest)

@pytest.fixture
def temp_aos_file(tmp_path):
    aos_path = tmp_path / "test.aos"
    create_minimal_aos(aos_path)
    return aos_path

def test_upload_success(client, auth_headers, temp_aos_file):
    """Test successful upload"""
    with open(temp_aos_file, 'rb') as f:
        response = client.post(
            '/v1/adapters/upload-aos',
            headers=auth_headers,
            data={'file': f, 'name': 'Test Adapter'},
            content_type='multipart/form-data'
        )

    assert response.status_code == 200
    data = response.get_json()
    assert 'adapter_id' in data
    assert 'hash_b3' in data
    assert data['lifecycle_state'] == 'draft'

def test_upload_no_file(client, auth_headers):
    """Test upload without file"""
    response = client.post(
        '/v1/adapters/upload-aos',
        headers=auth_headers,
        data={'name': 'Test Adapter'},
        content_type='multipart/form-data'
    )

    assert response.status_code == 400
    assert response.get_json()['error_code'] == 'AOS_INVALID_REQUEST'

def test_upload_wrong_extension(client, auth_headers, tmp_path):
    """Test upload with wrong file extension"""
    txt_file = tmp_path / "adapter.txt"
    txt_file.write_text("not aos")

    with open(txt_file, 'rb') as f:
        response = client.post(
            '/v1/adapters/upload-aos',
            headers=auth_headers,
            data={'file': f},
            content_type='multipart/form-data'
        )

    assert response.status_code == 400
    assert response.get_json()['error_code'] == 'AOS_INVALID_EXTENSION'
```

---

## Common Error Patterns

### Pattern: Upload works locally but fails in CI/CD

**Likely cause:** Path or permission differences

```yaml
# .github/workflows/deploy.yml
- name: Upload adapter
  env:
    API_URL: ${{ secrets.API_URL }}
    JWT_TOKEN: ${{ secrets.JWT_TOKEN }}
  run: |
    # Use absolute paths in CI
    ADAPTER_PATH="$(pwd)/build/adapter.aos"
    echo "Uploading from: $ADAPTER_PATH"

    curl -F "file=@$ADAPTER_PATH" \
      -H "Authorization: Bearer $JWT_TOKEN" \
      "$API_URL/v1/adapters/upload-aos"
```

### Pattern: Intermittent failures with large files

**Likely cause:** Timeout or memory pressure

```python
import time
from requests.adapters import HTTPAdapter

session = requests.Session()

# Configure retry strategy
from urllib3.util.retry import Retry
retry = Retry(
    total=5,
    backoff_factor=2.0,
    status_forcelist=[500, 502, 503, 504]
)
adapter = HTTPAdapter(max_retries=retry)
session.mount('http://', adapter)
session.mount('https://', adapter)

# Longer timeout for large files
timeout = 600  # 10 minutes

def upload_large_file_reliably(session, url, file_path):
    with open(file_path, 'rb') as f:
        return session.post(
            url,
            files={'file': f},
            timeout=timeout
        )
```

### Pattern: Batch uploads fail after X uploads

**Likely cause:** Rate limiting or connection pool exhaustion

```python
from threading import Semaphore
import time

class RateLimitedUploader:
    def __init__(self, max_concurrent=3, uploads_per_minute=60):
        self.semaphore = Semaphore(max_concurrent)
        self.delay = 60 / uploads_per_minute

    def upload(self, file_path, name):
        with self.semaphore:
            result = do_upload(file_path, name)
            time.sleep(self.delay)
            return result

# Usage
uploader = RateLimitedUploader(max_concurrent=2, uploads_per_minute=50)
for file_path in large_file_list:
    result = uploader.upload(file_path, get_name(file_path))
```

---

## Escalation Procedures

### When to Contact Support

Contact the AdapterOS team if:
1. Error persists after trying all troubleshooting steps
2. Server shows unusual resource usage
3. Error messages are cryptic or non-actionable
4. Multiple adapters affected simultaneously

### Information to Provide

```bash
# Gather diagnostics
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/system/info > system_info.json
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/adapters > adapters.json
dmesg > system_messages.log
tail -1000 /var/log/aos/server.log > server_logs.log

# Create diagnostic archive
tar czf aos_diagnostics_$(date +%s).tar.gz \
  system_info.json adapters.json system_messages.log server_logs.log

# Include:
# - system_info.json
# - server_logs.log
# - error messages (screenshot or copy-paste)
# - file size and format
# - network conditions (optional)
```

---

**Last Updated:** 2025-01-19
**Maintained By:** AdapterOS Team
