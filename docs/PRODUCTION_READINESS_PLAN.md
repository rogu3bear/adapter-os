# Production Readiness Plan

**Status:** 65/100 (Security Audit: 85/100)
**Target:** 95/100
**Last Updated:** 2024-12-15

---

## Executive Summary

AdapterOS has solid security foundations (tenant isolation, policy enforcement, deterministic execution) but requires several fixes before production deployment. This plan prioritizes issues by severity and provides clear action items.

---

## Phase 1: Critical Security Fixes (Block Production)

### 1.1 Remove Debug Logging to Hardcoded Path
**File:** `crates/adapteros-server/src/main.rs`
**Risk:** CRITICAL
**Effort:** 1 hour

The `agent_log()` function writes sensitive data to `/Users/mln-dev/Dev/adapter-os/.cursor/debug.log`:
- Database configuration
- Server startup parameters
- Environmental settings

**Action:**
```bash
# Remove all agent_log() calls and the function definition
grep -rn "agent_log" crates/adapteros-server/src/main.rs
# Delete lines 127-159 (function definition)
# Delete calls at lines 798, 821, 833
```

---

### 1.2 Fix Permissive CORS in Service Supervisor
**Files:**
- `crates/adapteros-service-supervisor/src/server.rs:65`
- `crates/adapteros-api/src/lib.rs`

**Risk:** HIGH
**Effort:** 2 hours

**Current (INSECURE):**
```rust
.layer(CorsLayer::permissive())
```

**Action:** Replace with restrictive CORS using the pattern from `middleware_security.rs`:
```rust
.layer(cors_layer())  // Use the validated cors_layer() function
```

Or add explicit configuration:
```rust
let origins = std::env::var("ALLOWED_ORIGINS")
    .map(|s| s.split(',').filter_map(|o| o.trim().parse().ok()).collect())
    .unwrap_or_default();

CorsLayer::new()
    .allow_origin(origins)
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
    .allow_credentials(true)
```

---

### 1.3 Restrict CLI Security Bypass Flags
**File:** `crates/adapteros-server/src/main.rs:185-191`
**Risk:** MEDIUM-HIGH
**Effort:** 1 hour

**Current:**
```rust
#[arg(long)]
skip_pf_check: bool,

#[arg(long)]
skip_drift_check: bool,
```

**Action:** Gate these flags to debug builds only:
```rust
#[cfg(debug_assertions)]
#[arg(long, help = "Skip PF/firewall checks (debug builds only)")]
skip_pf_check: bool,

#[cfg(not(debug_assertions))]
#[arg(skip)]
skip_pf_check: bool,
```

---

### 1.4 Fix Service Supervisor Keypair Handling
**File:** `crates/adapteros-service-supervisor/src/supervisor.rs`
**Risk:** MEDIUM-HIGH
**Effort:** 3 hours

**Current Issue:** Generates ephemeral keypairs on each startup, breaking token validation.

**Action:**
1. Implement PEM loading as documented
2. Use `var/keys/supervisor_signing.key` with self-healing pattern (like promotion signing)
3. Warn loudly if generating new key in production mode

---

## Phase 2: Production Configuration

### 2.1 Required Environment Variables
Create/update `.env.production`:

```bash
# === REQUIRED FOR PRODUCTION ===

# Authentication
AOS_PRODUCTION_MODE=true
JWT_SECRET=<64-char-random-string>  # Generate with: openssl rand -hex 32

# CORS - Comma-separated allowed origins
ALLOWED_ORIGINS=http://localhost:3000,http://localhost:8080

# Database
DATABASE_URL=sqlite:var/aos-cp.sqlite3?mode=rwc
SQLITE_POOL_SIZE=10

# === RECOMMENDED ===

# Logging (reduce verbosity for production)
RUST_LOG=info,adapteros=info,tower_http=warn

# Network binding (behind reverse proxy)
AOS_BIND_HOST=127.0.0.1
AOS_BIND_PORT=8080

# TLS (if not using reverse proxy)
# AOS_TLS_CERT=/path/to/cert.pem
# AOS_TLS_KEY=/path/to/key.pem

# Keys (auto-generated if not set)
# PROMOTION_SIGNING_KEY=<64-char-hex>
```

---

### 2.2 Configuration Validation on Startup
**File:** `crates/adapteros-server/src/main.rs`
**Effort:** 4 hours

Add startup validation:

```rust
fn validate_production_config() -> Result<(), String> {
    let is_prod = std::env::var("AOS_PRODUCTION_MODE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if !is_prod {
        return Ok(()); // Skip validation in dev mode
    }

    // JWT secret validation
    let jwt_secret = std::env::var("JWT_SECRET")
        .map_err(|_| "JWT_SECRET must be set in production")?;
    if jwt_secret.len() < 32 {
        return Err("JWT_SECRET must be at least 32 characters".into());
    }

    // CORS validation
    if std::env::var("ALLOWED_ORIGINS").is_err() {
        return Err("ALLOWED_ORIGINS must be set in production".into());
    }

    Ok(())
}
```

---

### 2.3 Reverse Proxy Configuration (Nginx)

```nginx
upstream adapteros {
    server 127.0.0.1:8080;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name app.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/yourdomain.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options DENY always;
    add_header X-Content-Type-Options nosniff always;

    location / {
        proxy_pass http://adapteros;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}

# HTTP to HTTPS redirect
server {
    listen 80;
    server_name localhost;
    return 301 https://$server_name$request_uri;
}
```

---

## Phase 3: Security Hardening

### 3.1 Complete Security Feature Implementations
**Effort:** 8-16 hours total

| Feature | Location | Priority |
|---------|----------|----------|
| Password rotation tracking | `auth_enhanced.rs` | Medium |
| Token rotation timestamps | `auth_enhanced.rs` | Medium |
| Last login tracking | User handlers | Low |
| Signature verification | `signing.rs` | High |

---

### 3.2 Add Security Headers Middleware
**File:** `crates/adapteros-server-api/src/middleware_security.rs`
**Status:** Partially implemented

Verify these headers are set:
- `Strict-Transport-Security`
- `X-Frame-Options: DENY`
- `X-Content-Type-Options: nosniff`
- `Content-Security-Policy`
- `Referrer-Policy: strict-origin-when-cross-origin`

---

### 3.3 Rate Limiting Enhancement
**Current:** Brute force protection (5-failure lockout)
**Enhancement:** Add per-endpoint rate limiting

```rust
// Add to routes.rs
.layer(RateLimitLayer::new(
    100,  // requests
    Duration::from_secs(60),  // per minute
))
```

---

## Phase 4: Operational Readiness

### 4.1 Monitoring Setup
```yaml
# Prometheus scrape config
scrape_configs:
  - job_name: 'adapteros'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'
```

### 4.2 Health Check Endpoints
- `GET /health` - Basic liveness
- `GET /health/ready` - Readiness (DB connected, workers available)
- `GET /health/live` - Kubernetes liveness probe

### 4.3 Backup Strategy
```bash
# Daily database backup
0 2 * * * sqlite3 /var/aos/aos-cp.sqlite3 ".backup /backups/aos-$(date +%Y%m%d).sqlite3"

# Keep 30 days
find /backups -name "aos-*.sqlite3" -mtime +30 -delete
```

### 4.4 Log Aggregation
Configure structured JSON logging for production:
```bash
RUST_LOG_FORMAT=json
```

---

## Pre-Deployment Checklist

### Security
- [ ] Remove `agent_log()` debug function
- [ ] Fix permissive CORS in service supervisor
- [ ] Restrict `--skip-pf-check` and `--skip-drift-check` flags
- [ ] Verify JWT secret is set and has sufficient entropy
- [ ] Verify `ALLOWED_ORIGINS` is configured
- [ ] Verify `AOS_PRODUCTION_MODE=true`
- [ ] Run `cargo build --release` (not debug)

### Configuration
- [ ] Create `.env.production` with all required vars
- [ ] Configure reverse proxy with TLS
- [ ] Set up database backups
- [ ] Configure log aggregation

### Testing
- [ ] Run full test suite: `cargo test --workspace`
- [ ] Run security tests: `cargo test -p adapteros-server-api security`
- [ ] Verify CORS blocks unauthorized origins
- [ ] Verify auth works end-to-end
- [ ] Load test critical endpoints

### Operations
- [ ] Set up monitoring/alerting
- [ ] Document runbooks for common issues
- [ ] Configure health check endpoints
- [ ] Test backup/restore procedure

---

## Timeline Estimate

| Phase | Effort | Priority |
|-------|--------|----------|
| Phase 1: Critical Fixes | 8 hours | MUST HAVE |
| Phase 2: Configuration | 4 hours | MUST HAVE |
| Phase 3: Hardening | 16 hours | SHOULD HAVE |
| Phase 4: Operations | 8 hours | SHOULD HAVE |

**Total:** ~36 hours for full production readiness

---

## Quick Start (Minimum Viable Production)

For fastest path to production, complete only:

1. Remove `agent_log()` (1 hour)
2. Fix CORS in supervisor (1 hour)
3. Set required env vars (30 min)
4. Deploy behind TLS reverse proxy (1 hour)
5. Verify with checklist (1 hour)

**Minimum Time:** ~5 hours
