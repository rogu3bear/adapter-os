# AdapterOS Environment Configuration Scripts

Quick-reference guide for environment setup and configuration scripts.

---

## Overview

Three essential scripts for managing your AdapterOS environment:

| Script | Purpose | Use When |
|--------|---------|----------|
| `setup_env.sh` | Interactive environment setup wizard | First-time setup or major configuration changes |
| `validate_env.sh` | Configuration validation and diagnostics | Verifying setup, troubleshooting issues |
| `switch_env_profile.sh` | Quick profile switching | Switching between dev/training/production |

---

## setup_env.sh

**Interactive setup wizard for first-time configuration.**

### Usage

```bash
./scripts/setup_env.sh
```

### What It Does

1. **Checks prerequisites:** Ensures `.env.example` exists
2. **Copies template:** Creates `.env` from `.env.example`
3. **Selects profile:** Asks about your use case (dev/training/prod)
4. **Auto-configures:** Sets up environment for selected profile
5. **Optional downloads:** Offers to download model
6. **Optional database:** Offers to initialize database
7. **Validates setup:** Runs validation check

### Output

```
╔════════════════════════════════════════════════════════════════╗
║ AdapterOS Environment Setup                                   ║
╚════════════════════════════════════════════════════════════════╝

→ Creating .env file
  ✓ .env file created from .env.example

→ Selecting setup profile
Select profile (1-4):
  1. Development
  2. Training
  3. Production
  4. Custom
```

### Supported Profiles

#### 1. Development
- Debug logging: `RUST_LOG=debug,adapteros=trace`
- Mode: Development (`AOS_SERVER_PRODUCTION_MODE=false`)
- JWT: Simple HMAC-SHA256
- Backend: Auto-select

#### 2. Training
- Backend: MLX (GPU-accelerated)
- Precision: float16 (GPU-optimized)
- Memory pool: Enabled
- Logging: Backend debug enabled

#### 3. Production
- Mode: Enforced security
- Backend: CoreML (ANE acceleration)
- JWT: Ed25519 (secure)
- Telemetry: Enabled
- Requires: UDS socket, database path, JWT secret

#### 4. Custom
- No auto-configuration
- You edit `.env` manually
- Full control over settings

### Time Required

- **Automated:** 5-10 minutes
- **With model download:** 10-15 minutes
- **With database init:** 15-20 minutes

### Example Session

```bash
$ ./scripts/setup_env.sh

╔════════════════════════════════════════════════════════════════╗
║ AdapterOS Environment Setup                                   ║
╚════════════════════════════════════════════════════════════════╝

Creating optimized environment configuration for your workflow

This script will:
  1. Copy .env.example to .env
  2. Ask about your setup (development, training, or production)
  3. Configure the environment for your use case
  4. Validate your configuration

Continue with environment setup? [Y/n] y

→ Creating .env file
  ✓ .env file created from .env.example

→ Selecting setup profile
Choose your use case:
  1. Development - Local testing, all features, debug logging
  2. Training - MLX backend, GPU acceleration, fine-tuning
  3. Production - CoreML backend, maximum security, auditing
  4. Custom - Manual configuration

Select profile (1-4): 1

→ Configuring for development
  ✓ Development settings configured

Configuration summary:
  • Debug logging enabled (RUST_LOG=debug)
  • Development mode (AOS_SERVER_PRODUCTION_MODE=false)
  • HMAC-SHA256 JWT (HS256)
  • Auto backend selection (CoreML > Metal > MLX)

→ Setting up model
Download model now? (required for inference) [Y/n] y
Model downloaded to: models/qwen2.5-7b-mlx

→ Setting up database
Initialize database now? [Y/n] y
  ✓ Migrations complete
  ✓ Default tenant created

→ Validating configuration
[PASS] Configuration validation successful

╔════════════════════════════════════════════════════════════════╗
║ Setup Complete                                                ║
╚════════════════════════════════════════════════════════════════╝

Your environment is configured! Next steps:

1. Start the backend server:
   cargo run --release -p adapteros-server-api

2. In another terminal, start the UI:
   cd ui && pnpm install && pnpm dev

3. Access the UI at http://localhost:3200

✅ Setup complete. Happy coding!
```

---

## validate_env.sh

**Comprehensive configuration validation and diagnostics.**

### Usage

```bash
./scripts/validate_env.sh
```

### What It Checks

1. **File existence:** `.env` file present
2. **Model configuration:** `AOS_MODEL_PATH` set, model files exist
3. **Backend selection:** Valid backend specified
4. **Server configuration:** Port available, settings valid
5. **Database:** URL valid, directory accessible
6. **Security:** JWT mode valid, credentials present
7. **Tools:** Rust, Cargo, Node.js, pnpm available
8. **Directories:** Required directories exist
9. **Production checks:** Additional requirements if production mode

### Output Sections

```
╔════════════════════════════════════════════════════════════════╗
║ AdapterOS Configuration Validation                            ║
╚════════════════════════════════════════════════════════════════╝

=== MODEL CONFIGURATION ===
[CHECK] AOS_MODEL_PATH is set
[PASS] AOS_MODEL_PATH=./models/qwen2.5-7b-mlx
[PASS] Model config.json found

[CHECK] AOS_MODEL_BACKEND is set
[PASS] AOS_MODEL_BACKEND=auto

=== SERVER CONFIGURATION ===
[CHECK] AOS_SERVER_PORT is set
[PASS] AOS_SERVER_PORT=8080
[CHECK] Port 8080 is available
[PASS] Port 8080 is available

=== DATABASE CONFIGURATION ===
[CHECK] AOS_DATABASE_URL is set
[PASS] AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3
[CHECK] Database directory exists
[PASS] Database directory found

=== TOOL AVAILABILITY ===
[CHECK] Rust compiler is available
[PASS] Rust: rustc 1.81.0

[CHECK] Cargo is available
[PASS] cargo 1.81.0

[CHECK] Node.js is available
[PASS] Node.js: v20.14.0

[CHECK] pnpm is available
[PASS] pnpm: 9.6.0

═══════════════════════════════════════════════════════════════════════════════
VALIDATION SUMMARY
═══════════════════════════════════════════════════════════════════════════════
Checks run: 25
Passed: 25
Warnings: 0
Errors: 0

✅ Environment validation passed
Ready to start AdapterOS!
```

### Exit Codes

- `0` - All checks passed (or passed with warnings)
- `1` - Validation failed with errors

### Example: With Errors

```bash
$ ./scripts/validate_env.sh

[CHECK] .env file exists
[FAIL] .env file not found. Run: cp .env.example .env

[CHECK] AOS_MODEL_PATH is set
[FAIL] AOS_MODEL_PATH is not set

═══════════════════════════════════════════════════════════════════════════════
VALIDATION SUMMARY
═══════════════════════════════════════════════════════════════════════════════
Checks run: 5
Passed: 0
Warnings: 0
Errors: 2

❌ Environment validation FAILED
Fix the errors above and re-run this script

$ echo $?
1  # Exit code 1 indicates failure
```

---

## switch_env_profile.sh

**Quick switching between configuration profiles.**

### Usage

```bash
# Switch to development profile
./scripts/switch_env_profile.sh dev

# Switch to training profile
./scripts/switch_env_profile.sh training

# Switch to production profile
./scripts/switch_env_profile.sh prod

# Show current settings
./scripts/switch_env_profile.sh show

# Show usage help
./scripts/switch_env_profile.sh
```

### What It Does

1. **Checks `.env` exists:** Ensures configuration file is present
2. **Updates variables:** Modifies `.env` with profile settings
3. **Validates:** Runs quick configuration check
4. **Shows summary:** Displays what was changed

### Supported Profiles

#### `dev` (Development)
```bash
./scripts/switch_env_profile.sh dev
```

Changes:
- `RUST_LOG=debug,adapteros=trace` - Debug logging
- `AOS_SERVER_PRODUCTION_MODE=false` - Development mode
- `AOS_SECURITY_JWT_MODE=hs256` - Simple JWT
- `AOS_MODEL_BACKEND=auto` - Auto-select backend

Good for: Local testing, debugging

#### `training` (Training)
```bash
./scripts/switch_env_profile.sh training
```

Changes:
- `AOS_MODEL_BACKEND=mlx` - MLX backend
- `AOS_MLX_PRECISION=float16` - GPU optimization
- `AOS_MLX_MEMORY_POOL_ENABLED=true` - Memory efficiency
- `RUST_LOG=info,adapteros_lora_mlx_ffi=debug` - Backend debug

Good for: ML fine-tuning, experiments

#### `prod` (Production)
```bash
./scripts/switch_env_profile.sh prod
```

Changes:
- `AOS_SERVER_PRODUCTION_MODE=true` - Enforced security
- `AOS_MODEL_BACKEND=coreml` - CoreML/ANE
- `AOS_SECURITY_JWT_MODE=eddsa` - Ed25519 signing
- `AOS_SECURITY_PF_DENY=true` - PF deny rules
- `AOS_TELEMETRY_ENABLED=true` - Audit logging

Good for: Production serving

### Example Session

```bash
$ ./scripts/switch_env_profile.sh training

╔════════════════════════════════════════════════════════════════╗
║ Switching to TRAINING PROFILE
╚════════════════════════════════════════════════════════════════╝

Configuration updated:

  • Backend: MLX (GPU-accelerated)
  • Precision: float16 (optimized)
  • Memory: Pool enabled
  • Log level: DEBUG for MLX

Good for: Training LoRA adapters, ML experiments

Next: cargo run --release -p adapteros-server-api

Validating configuration...
[CHECK] AOS_MODEL_BACKEND is set
[PASS] AOS_MODEL_BACKEND=mlx
...
```

### Viewing Current Settings

```bash
$ ./scripts/switch_env_profile.sh show

Current Environment Settings:

Production Mode:
  AOS_SERVER_PRODUCTION_MODE=false

Security:
  AOS_SECURITY_JWT_MODE=hs256
  AOS_SECURITY_PF_DENY=false

Backend:
  AOS_MODEL_BACKEND=auto

Logging:
  RUST_LOG=debug,adapteros=trace

Database:
  AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3
```

---

## Workflow Examples

### New Developer Setup

```bash
# Step 1: Run interactive setup
./scripts/setup_env.sh

# Step 2: Start developing
cargo run --release -p adapteros-server-api

# Step 3: Access UI (in another terminal)
cd ui && pnpm dev
```

### Switching Between Use Cases

```bash
# Development work
./scripts/switch_env_profile.sh dev
cargo run --release -p adapteros-server-api

# Training work
./scripts/switch_env_profile.sh training
cargo run --release -p adapteros-server-api

# Back to development
./scripts/switch_env_profile.sh dev
```

### Troubleshooting Setup Issues

```bash
# Check what's configured
./scripts/switch_env_profile.sh show

# Validate entire setup
./scripts/validate_env.sh

# Fix issues and re-validate
vim .env
./scripts/validate_env.sh
```

### Production Deployment

```bash
# Switch to production profile
./scripts/switch_env_profile.sh prod

# Follow checklist in ENVIRONMENT_SETUP.md
# Verify configuration
./scripts/validate_env.sh

# Build and deploy
cargo build --release -p adapteros-server-api
```

---

## Troubleshooting

### Script Not Found

```bash
# Make scripts executable
chmod +x ./scripts/setup_env.sh
chmod +x ./scripts/validate_env.sh
chmod +x ./scripts/switch_env_profile.sh

# Or all at once
chmod +x ./scripts/*.sh
```

### Script Errors

```bash
# Check for issues
bash -x ./scripts/setup_env.sh  # Verbose mode

# Validate bash syntax
bash -n ./scripts/setup_env.sh  # No-execute mode
```

### .env File Issues

```bash
# View current settings
cat .env

# Reset to template
cp .env.example .env

# Validate
./scripts/validate_env.sh
```

---

## Documentation References

- **Full environment guide:** `docs/ENVIRONMENT_SETUP.md`
- **Configuration index:** `docs/ENVIRONMENT_CONFIGURATION_INDEX.md`
- **Quick reference:** `ENVIRONMENT_QUICK_REFERENCE.md`
- **Configuration template:** `.env.example`

---

## Related Commands

### Before Using Scripts

```bash
# View template
cat .env.example

# Copy template
cp .env.example .env

# Make scripts executable
chmod +x scripts/*.sh
```

### After Using Scripts

```bash
# Start server
cargo run --release -p adapteros-server-api

# Test connection
curl http://localhost:8080/api/healthz

# View logs
RUST_LOG=debug cargo run --release -p adapteros-server-api
```

---

## Tips & Tricks

### Quick Profile Switching

```bash
# Create aliases for faster switching
alias aos-dev='./scripts/switch_env_profile.sh dev'
alias aos-training='./scripts/switch_env_profile.sh training'
alias aos-prod='./scripts/switch_env_profile.sh prod'
alias aos-check='./scripts/validate_env.sh'

# Then use
aos-training
aos-check
```

### Keeping Variables Organized

```bash
# View only set variables
grep -v '^#' .env | grep -v '^$'

# Count variables
grep -c '^[A-Z_]=' .env

# Search for specific setting
grep AOS_MODEL .env
```

### Backup Before Major Changes

```bash
# Before switching profiles
cp .env .env.backup

# Restore if needed
cp .env.backup .env
```

---

**Last Updated:** 2025-11-23
**Related Documentation:** [ENVIRONMENT_SETUP.md](../docs/ENVIRONMENT_SETUP.md)
