# Authentication Deployment Guide

## Pre-Deployment Checklist

Before deploying AdapterOS to production, complete these authentication security checks:

- [ ] JWT mode configured (`EdDSA` recommended for production)
- [ ] Strong JWT secret generated or keypair configured
- [ ] Authentication mode set to `production`
- [ ] Development tokens disabled or removed
- [ ] HTTPS enabled and enforced
- [ ] CORS origins restricted to production domains
- [ ] Rate limiting enabled
- [ ] Token expiry configured appropriately (recommended: 8 hours)
- [ ] Security logging enabled
- [ ] Failed login monitoring configured

**Citations:**
- `crates/adapteros-server-api/src/state.rs` L170-242: Authentication configuration
- `docs/AUTHENTICATION.md`: Comprehensive security guide

## Environment Setup

### Development Environment

**Purpose**: Local development and testing

**Configuration** (`configs/cp-dev.toml`):
```toml
[auth]
mode = "development"
dev_token = "adapteros-local"
token_expiry_hours = 24  # Longer for convenience
max_login_attempts = 10
lockout_duration_minutes = 5

[security]
require_https = false
cors_origins = [
    "http://localhost:3200",
    "http://localhost:3201",
    "http://127.0.0.1:3200"
]
enable_rate_limiting = false  # Disable for easier testing
```

**Starting the Server**:
```bash
cd /Users/star/Dev/adapter-os

# Build the server
cargo build --release

# Start with development config
./target/release/adapteros-server \
  --skip-pf-check \
  --config configs/cp-dev.toml
```

**Starting the UI**:
```bash
cd ui
pnpm install
pnpm dev
```

**Citation**: `configs/cp.toml` (template)

### Staging Environment

**Purpose**: Pre-production testing with production-like settings

**Configuration** (`configs/cp-staging.toml`):
```toml
[auth]
mode = "mixed"  # Allows both dev tokens and real JWTs
dev_token = "adapteros-staging-key"  # Different from dev
token_expiry_hours = 8
max_login_attempts = 5
lockout_duration_minutes = 15

[security]
require_https = true
cors_origins = [
    "https://staging.adapteros.example.com"
]
enable_rate_limiting = true
```

**Deployment Steps**:
```bash
# 1. Build optimized binary
cargo build --release

# 2. Generate JWT secret (HMAC mode)
openssl rand -base64 32 > var/jwt_secret_staging.key

# OR generate keypair (EdDSA mode - recommended)
openssl genpkey -algorithm Ed25519 -out var/jwt_private_staging.pem
openssl pkey -in var/jwt_private_staging.pem -pubout -out var/jwt_public_staging.pem

# 3. Start server with staging config
./target/release/adapteros-server \
  --config configs/cp-staging.toml

# 4. Test authentication
curl -k https://staging.adapteros.example.com/healthz
```

**Citation**: `crates/adapteros-server-api/src/auth.rs` L46-86: JWT generation

### Production Environment

**Purpose**: Live deployment with maximum security

**Configuration** (`configs/cp-production.toml`):
```toml
[auth]
mode = "production"  # Strict JWT only
# NO dev_token configured
token_expiry_hours = 8
max_login_attempts = 5
lockout_duration_minutes = 30  # Longer lockout for security

[security]
require_https = true
cors_origins = [
    "https://app.adapteros.example.com",
    "https://console.adapteros.example.com"
]
enable_rate_limiting = true
```

**Deployment Steps**:

```bash
# 1. Build optimized release binary
cargo build --release --features production

# 2. Generate production JWT keypair (EdDSA recommended)
openssl genpkey -algorithm Ed25519 -out var/jwt_private.pem
openssl pkey -in var/jwt_private.pem -pubout -out var/jwt_public.pem

# Set restrictive permissions
chmod 600 var/jwt_private.pem
chmod 644 var/jwt_public.pem

# 3. Set environment variables
export AOS_AUTH_MODE=production
export AOS_JWT_PRIVATE_KEY_FILE=var/jwt_private.pem
export AOS_JWT_PUBLIC_KEY_FILE=var/jwt_public.pem
export AOS_REQUIRE_HTTPS=true

# 4. Start server (with process manager)
./target/release/adapteros-server \
  --config configs/cp-production.toml \
  2>&1 | tee -a /var/log/adapteros/server.log
```

**Citation**: `crates/adapteros-server-api/src/middleware.rs` L48-67: Production auth

## JWT Configuration

### HMAC Mode (Simple, Shared Secret)

**Pros**: Simple setup, good for single-server deployments  
**Cons**: Shared secret must be protected, less secure

**Setup**:
```bash
# Generate secret
openssl rand -base64 32 > var/jwt_secret.key

# Set in config
[jwt]
mode = "hmac"
secret_file = "var/jwt_secret.key"
```

**Citation**: `crates/adapteros-server-api/src/auth.rs` L88-126

### EdDSA Mode (Public Key, More Secure)

**Pros**: Public/private key pair, more secure, better for distributed systems  
**Cons**: Slightly more complex setup

**Setup**:
```bash
# Generate Ed25519 keypair
openssl genpkey -algorithm Ed25519 -out var/jwt_private.pem
openssl pkey -in var/jwt_private.pem -pubout -out var/jwt_public.pem

# Set in config
[jwt]
mode = "eddsa"
private_key_file = "var/jwt_private.pem"
public_key_file = "var/jwt_public.pem"
```

**Citation**: `crates/adapteros-server-api/src/auth.rs` L46-86

**Recommended**: EdDSA mode for production deployments

## Security Hardening

### 1. HTTPS Configuration

**Nginx Reverse Proxy** (`/etc/nginx/sites-available/adapteros`):
```nginx
server {
    listen 443 ssl http2;
    server_name app.adapteros.example.com;

    ssl_certificate /etc/letsencrypt/live/adapteros.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/adapteros.example.com/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;

    location / {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

# Redirect HTTP to HTTPS
server {
    listen 80;
    server_name app.adapteros.example.com;
    return 301 https://$server_name$request_uri;
}
```

### 2. Rate Limiting

Configure rate limiting in `configs/cp-production.toml`:
```toml
[security.rate_limiting]
enabled = true
requests_per_minute = 60
burst = 10
```

### 3. Firewall Rules

```bash
# Allow only HTTPS traffic
sudo ufw allow 443/tcp

# Allow SSH (change port as needed)
sudo ufw allow 22/tcp

# Deny all other incoming
sudo ufw default deny incoming
sudo ufw default allow outgoing

# Enable firewall
sudo ufw enable
```

### 4. Security Event Logging

All authentication events are logged with structured data:

**Failed Login**:
```json
{
  "level": "warn",
  "timestamp": "2024-10-20T22:15:30Z",
  "message": "Authentication attempt failed",
  "user_id": null,
  "ip_address": "192.168.1.100",
  "error": "invalid credentials"
}
```

**Token Refresh**:
```json
{
  "level": "info",
  "timestamp": "2024-10-20T22:20:15Z",
  "message": "Token refreshed successfully",
  "user_id": "user-123",
  "email": "user@example.com"
}
```

**Citation**: `crates/adapteros-server-api/src/handlers.rs` L2247-2310

## Monitoring

### Health Checks

```bash
# Server health
curl https://app.adapteros.example.com/healthz

# Expected response: 200 OK
```

### Authentication Metrics

Monitor these key metrics:

1. **Failed Login Attempts**: Track unusual patterns
2. **Token Refresh Rate**: Should be consistent
3. **401 Error Rate**: Indicates auth issues
4. **Token Expiry Events**: Track expired sessions

**Log Queries** (assuming structured logging to file):
```bash
# Failed logins in last hour
grep "Authentication attempt failed" /var/log/adapteros/server.log | \
  grep "$(date -u -d '1 hour ago' '+%Y-%m-%d %H')" | \
  wc -l

# Successful logins today
grep "Login attempt for email" /var/log/adapteros/server.log | \
  grep "$(date -u '+%Y-%m-%d')" | \
  wc -l
```

## Backup and Recovery

### Backup JWT Keys

```bash
# Create backup directory
mkdir -p /backup/adapteros/jwt-keys/$(date +%Y%m%d)

# Backup keys
cp var/jwt_private.pem /backup/adapteros/jwt-keys/$(date +%Y%m%d)/
cp var/jwt_public.pem /backup/adapteros/jwt-keys/$(date +%Y%m%d)/

# Encrypt backup (recommended)
tar czf - /backup/adapteros/jwt-keys/$(date +%Y%m%d)/ | \
  openssl enc -aes-256-cbc -salt -out /backup/adapteros/jwt-keys-$(date +%Y%m%d).tar.gz.enc
```

### Key Rotation

To rotate JWT keys without downtime:

1. Generate new keypair
2. Configure old public key for validation
3. Configure new private key for signing
4. Wait for all old tokens to expire
5. Remove old public key

**Note**: Full key rotation support requires implementation of multi-key validation.

## Troubleshooting

### Issue: "unauthorized" errors in production

**Symptoms**: All requests return 401 unauthorized

**Causes**:
1. JWT secret/keypair misconfigured
2. Auth mode set incorrectly
3. CORS issues
4. Token format issues

**Solutions**:
```bash
# 1. Verify configuration
cat configs/cp-production.toml | grep -A5 "\[auth\]"

# 2. Check JWT key files exist and have correct permissions
ls -l var/jwt_*.pem

# 3. Test with curl
curl -v https://app.adapteros.example.com/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin@example.com", "password": "password"}'

# 4. Check server logs
tail -f /var/log/adapteros/server.log | grep -i auth
```

### Issue: Token refresh not working

**Symptoms**: Users get logged out frequently

**Causes**:
1. Token expiry too short
2. Refresh endpoint not configured
3. Client-side refresh logic failing

**Solutions**:
```bash
# 1. Check token expiry configuration
grep token_expiry_hours configs/cp-production.toml

# 2. Test refresh endpoint
TOKEN="your-valid-token"
curl -X POST https://app.adapteros.example.com/api/v1/auth/refresh \
  -H "Authorization: Bearer $TOKEN"

# 3. Check browser console for refresh errors
# Open DevTools → Console → Filter by "refresh"
```

**Citation**: `ui/src/api/client.ts` L125-181

### Issue: CORS errors in browser

**Symptoms**: Browser shows CORS policy errors

**Causes**:
1. CORS origins not configured correctly
2. HTTPS vs HTTP mismatch
3. Port mismatch

**Solutions**:
```toml
# Update configs/cp-production.toml
[security]
cors_origins = [
    "https://app.adapteros.example.com",
    "https://app.adapteros.example.com:443"  # Include port if needed
]
```

**Citation**: `crates/adapteros-server-api/src/routes.rs` L682

## Rollback Procedure

If authentication issues occur in production:

```bash
# 1. Stop current server
sudo systemctl stop adapteros

# 2. Restore previous configuration
cp /backup/configs/cp-production.toml.backup configs/cp-production.toml

# 3. Restore JWT keys if needed
cp /backup/adapteros/jwt-keys/YYYYMMDD/* var/

# 4. Restart server
sudo systemctl start adapteros

# 5. Verify health
curl https://app.adapteros.example.com/healthz

# 6. Monitor logs
tail -f /var/log/adapteros/server.log
```

## Migration from Development to Production

1. **Update Configuration**:
   ```bash
   cp configs/cp-dev.toml configs/cp-production.toml
   # Edit cp-production.toml:
   # - Change mode to "production"
   # - Remove dev_token
   # - Enable require_https
   # - Update cors_origins
   ```

2. **Generate Production Keys**:
   ```bash
   openssl genpkey -algorithm Ed25519 -out var/jwt_private.pem
   openssl pkey -in var/jwt_private.pem -pubout -out var/jwt_public.pem
   chmod 600 var/jwt_private.pem
   ```

3. **Update Frontend**:
   ```bash
   cd ui
   # Update .env.production
   echo "VITE_API_URL=https://app.adapteros.example.com/api" > .env.production
   
   # Build production bundle
   pnpm build
   ```

4. **Deploy**:
   ```bash
   # Server
   cargo build --release
   sudo systemctl start adapteros
   
   # Frontend (serve dist/ with nginx or similar)
   sudo cp -r ui/dist/* /var/www/adapteros/
   ```

5. **Verify**:
   ```bash
   curl https://app.adapteros.example.com/healthz
   curl https://app.adapteros.example.com/api/v1/meta
   ```

## Best Practices

1. **Never commit secrets**: Use `.gitignore` for `var/` directory
2. **Rotate keys regularly**: Recommended every 90 days
3. **Monitor authentication metrics**: Set up alerts for anomalies
4. **Use strong passwords**: Enforce password policies
5. **Enable MFA**: When implemented
6. **Log security events**: Comprehensive audit trail
7. **Regular security audits**: Review configurations quarterly
8. **Backup keys securely**: Encrypted off-site storage
9. **Test disaster recovery**: Practice key rotation procedures
10. **Document incidents**: Learn from security events

## References

- **Authentication Architecture**: `docs/AUTHENTICATION.md`
- **Security Policies**: Policy Pack #1 (Egress), Policy Pack #9 (Telemetry)
- **API Documentation**: Swagger UI at `/swagger-ui`
- **Troubleshooting**: `ui/TROUBLESHOOTING.md`
- **Contributing Guidelines**: `CONTRIBUTING.md`

**Code Citations**:
- `crates/adapteros-server-api/src/state.rs`: Configuration structures
- `crates/adapteros-server-api/src/middleware.rs`: Authentication middleware
- `crates/adapteros-server-api/src/auth.rs`: JWT logic
- `crates/adapteros-server-api/src/errors.rs`: Error handling
- `ui/src/api/client.ts`: Frontend authentication client

