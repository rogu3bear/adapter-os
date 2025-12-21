.PHONY: help build prepare test test-rust test-ui test-e2e test-ignored test-hw clean fmt fmt-check clippy metal ui ui-dev menu-bar menu-bar-dev menu-bar-install infra-check dev dev-no-auth build-mlx test-mlx bench-mlx verify-mlx-env cli setup-git-hooks lint-fix mvp-demo stability-check stability-ci ignored-tests-audit ignored-tests-check

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build all crates (fresh build with cleanup)
	@echo "🧹 Performing fresh build (stopping services, cleaning ports)..."
	./scripts/fresh-build.sh
	@echo "🔄 Building all crates..."
	cargo build --release --locked --offline
	./scripts/build_metadata.sh
	./scripts/record_env.sh
	./scripts/strip_timestamps.sh
	@echo "✅ Fresh build complete!"

cli: ## Build CLI with TUI and symlink to ./aosctl
	@echo "🔧 Building aosctl CLI with TUI..."
	cargo build --release -p adapteros-cli --features tui
	@ln -sf target/release/aosctl ./aosctl
	@echo "✅ CLI ready! Run with: ./aosctl"
	@echo "   TUI dashboard: ./aosctl tui"

dev: ## Run control plane in dev mode (auth bypass available, no prod hardening). Set NO_AUTH=1 to disable auth middleware.
	@echo "🧹 Cleaning ports for dev server..."
	-@lsof -ti:8080 | xargs kill -9 2>/dev/null || true
	@echo "🚀 Starting dev server..."
	AOS_DEV_NO_AUTH=$(NO_AUTH) cargo run -p adapteros-server -- --config configs/cp.toml

dev-no-auth: ## Run control plane in dev mode with authentication disabled (debug-only)
	NO_AUTH=1 $(MAKE) dev

prepare: ## Prepare build environment: stop services, clean ports
	@echo "🧹 Preparing build environment (stopping services, cleaning ports)..."
	./scripts/fresh-build.sh
	@echo "✅ Build environment ready!"

download-model: ## Download Qwen 2.5 7B Instruct model (~3.8GB)
	@./scripts/download-model.sh

mvp-demo: ## One-command MVP demo setup (deps, model, build, db, demo data)
	@./scripts/mvp-demo-setup.sh

check-system: ## Check system readiness before launch (preflight checks)
	@./scripts/check-system.sh

test: ## Run formatter, lint, Rust, and UI test suites
	bash scripts/test/all.sh all

test-rust: ## Run formatter, lint, Rust unit/integration tests only
	bash scripts/test/all.sh rust

test-ui: ## Run UI lint + unit/integration tests only
	bash scripts/test/all.sh ui

test-e2e: ## Run UI end-to-end tests (optional, starts dev stack)
	bash scripts/test/all.sh e2e

IGNORED_EXCLUDE ?=
IGNORED_EXCLUDE_ARGS := $(foreach ex,$(IGNORED_EXCLUDE),--exclude $(ex))
IGNORED_FEATURES ?= extended-tests
IGNORED_FEATURES_ARGS := $(if $(strip $(IGNORED_FEATURES)),--features $(IGNORED_FEATURES),)
test-ignored: ## Run ignored Rust tests (non-blocking suite, requires tracking IDs in ignore reasons)
	@echo "=== Running Ignored Tests (Non-Blocking) ==="
	@echo "These tests require infrastructure, pending APIs, or external dependencies"
	@echo "All ignored tests must have tracking IDs: [tracking: STAB-IGN-XXXX]"
	@echo ""
	cargo test --workspace $(IGNORED_EXCLUDE_ARGS) $(IGNORED_FEATURES_ARGS) --lib --bins --examples -- --ignored
	cargo test --workspace $(IGNORED_EXCLUDE_ARGS) $(IGNORED_FEATURES_ARGS) --tests -- --ignored
	@echo ""
	@echo "=== Ignored Tests Complete ==="
	@echo "Review failures in docs/stability/IGNORED_TESTS.md before release"

HW_PROFILE ?= release
HW_ROOT_FEATURES ?= hardware-residency
HW_WORKER_FEATURES ?= hardware-residency,ci-residency
test-hw: ## Run hardware-dependent tests (non-blocking, requires macOS with Metal GPU)
	@echo "=== Running Hardware Tests (Non-Blocking) ==="
	@echo "Requires: macOS with Metal GPU, signed kernel libraries"
	@echo "Cannot run in CI - for local validation only"
	@echo ""
	@echo "Metal LoRA buffer population tests..."
	cargo test --test lora_buffer_population_integration --features extended-tests --profile $(HW_PROFILE) -- --ignored --nocapture
	@echo ""
	@echo "KV residency and quota integration tests..."
	cargo test --test kv_residency_quota_integration --features $(HW_ROOT_FEATURES)
	@echo ""
	@echo "Worker enforcement and residency probe tests..."
	cargo test -p adapteros-lora-worker --features $(HW_WORKER_FEATURES) --test worker_enforcement_tests
	cargo test -p adapteros-lora-worker --features $(HW_WORKER_FEATURES) --test residency_probe
	@echo ""
	@echo "CoreML kernel integration tests..."
	cargo test -p adapteros-lora-kernel-coreml --test integration_tests -- --ignored
	@echo ""
	@echo "Metal heap observer tests..."
	cargo test -p adapteros-memory --test metal_heap_tests --profile $(HW_PROFILE) -- --ignored
	cargo test -p adapteros-memory --lib --profile $(HW_PROFILE) -- --ignored
	@echo ""
	@echo "=== Hardware Tests Complete ==="
	@echo "All tests use tracking IDs from docs/stability/IGNORED_TESTS.md"
clean: ## Clean build artifacts
	cargo clean
	rm -f metal/*.air metal/*.metallib
	rm -rf dist node_modules
	rm -rf crates/mplora-server/static

infra-check: ## Run infrastructure health checks (prevents rectification issues)
	./scripts/prevent_infrastructure_issues.sh
	cd menu-bar-app && swift package clean || true

security-audit: ## Run comprehensive security audit (vulnerabilities, licenses, SBOM)
	bash scripts/security_audit.sh

license-check: ## Check dependency license compliance
	@cargo install cargo-license --quiet 2>/dev/null || true
	@cargo license --json > var/security/licenses-$(shell date +%Y%m%d-%H%M%S).json
	@echo "✅ License report generated in var/security/"

fmt: ## Format code
	cargo fmt --all

fmt-check: ## Check formatting without modifying files
	cargo fmt --all --check

clippy: ## Run clippy (with smart test/example suppression via clippy.toml)
	cargo clippy --all-features -- -D warnings

metal: ## Build Metal shaders
	cd metal && bash build.sh

ui: ## Build Web UI (production)
	pnpm build

ui-dev: ## Start Web UI dev server
	pnpm dev

codegraph-viewer: ## Build CodeGraph Viewer (Tauri desktop app)
	cd crates/mplora-codegraph-viewer/frontend && pnpm install && pnpm build
	cd crates/mplora-codegraph-viewer/src-tauri && cargo build --release

codegraph-viewer-dev: ## Start CodeGraph Viewer in dev mode
	cd crates/mplora-codegraph-viewer/frontend && pnpm install
	cd crates/mplora-codegraph-viewer/src-tauri && cargo tauri dev

# Docker commands
docker-build: ## Build production Docker image
	docker build -t adapteros:latest .

docker-dev: ## Start development environment with docker-compose
	docker-compose --profile dev up -d

docker-test: ## Start test environment with docker-compose
	docker-compose --profile postgres up -d

docker-monitoring: ## Start monitoring stack with docker-compose
	docker-compose --profile monitoring up -d

docker-down: ## Stop all docker-compose services
	docker-compose down -v

docker-clean: ## Remove all Docker images and volumes
	docker-compose down -v --rmi all
	docker system prune -f

# Deployment commands
terraform-init: ## Initialize Terraform
	cd terraform/aws && terraform init

terraform-plan: ## Plan Terraform changes
	cd terraform/aws && terraform plan

terraform-apply: ## Apply Terraform changes
	cd terraform/aws && terraform apply

deploy-staging: ## Deploy to staging environment
	@echo "Triggering staging deployment..."
	@gh workflow run deploy.yml -f environment=staging

deploy-prod: ## Deploy to production environment (requires manual approval)
	@echo "⚠️  Production deployment requires manual approval"
	@echo "Run: git commit --allow-empty -m '[deploy prod] Deploy to production'"
	@echo "Then push to main branch"

setup-git-hooks: ## Setup git hooks for code quality (run once after cloning)
	@echo "🔧 Setting up git hooks..."
	@./scripts/setup-git-hooks.sh

lint-fix: ## Auto-fix common linting issues (unused imports, formatting, etc.)
	@echo "🔧 Auto-fixing lint issues..."
	cargo clippy --fix --allow-dirty --allow-staged
	cargo fmt --all
	@echo "✅ Auto-fix complete! Run 'make clippy' to check remaining issues."

check: fmt clippy test determinism-check ## Run all checks

stability-ci: ## Feature matrix build (defaults + all-features)
	./scripts/ci/feature_matrix.sh

stability-check: ## Must-pass stabilization gate (see docs/stability/CHECKLIST.md)
	@echo "=== Stability Gate ==="
	@echo "Step 1/3: Inference bypass guard..."
	./scripts/check_inference_bypass.sh
	@echo "Step 2/3: Full test suite (fmt, clippy, Rust tests, UI tests)..."
	@$(MAKE) test
	@echo "Step 3/3: Determinism checks..."
	@$(MAKE) determinism-check
	@echo "=== Stability Gate Passed ==="

install: build ## Install aosctl
	cargo install --path crates/aos-cli

installer: ## Build graphical SwiftUI installer
	cd installer && xcodebuild -project AdapterOSInstaller.xcodeproj -scheme AdapterOSInstaller -configuration Release -derivedDataPath build clean build

installer-open: ## Open installer in Xcode
	open installer/AdapterOSInstaller.xcodeproj

menu-bar: ## Build menu bar app
	cd menu-bar-app && swift build -c release

menu-bar-dev: ## Build and run menu bar app (debug)
	cd menu-bar-app && swift run

menu-bar-install: menu-bar ## Install menu bar app to /usr/local/bin
	cp menu-bar-app/.build/release/AdapterOSMenu /usr/local/bin/aos-menu
	@echo "Menu bar app installed to /usr/local/bin/aos-menu"

sbom: ## Generate SBOM
	cargo xtask sbom

determinism-report: ## Generate determinism report
	cargo xtask determinism-report

verify-artifacts: ## Verify and sign artifacts
	cargo xtask verify-artifacts

openapi-docs: ## Generate OpenAPI documentation
	cargo xtask openapi-docs

validate-openapi: ## Validate OpenAPI documentation
	./scripts/validate_openapi_docs.sh

gen-types: ## Generate TypeScript types from OpenAPI spec
	@echo "🔧 Generating TypeScript types from OpenAPI spec..."
	./scripts/generate-sdks.sh --typescript
	@echo "✅ TypeScript types generated at ui/src/api/generated.ts"

gen-sdk-python: ## Generate Python SDK from OpenAPI spec
	@echo "🔧 Generating Python SDK from OpenAPI spec..."
	./scripts/generate-sdks.sh --python
	@echo "✅ Python SDK generated at sdk/python/"

gen-sdks: ## Generate all SDKs (TypeScript types + Python SDK)
	@echo "🔧 Generating all SDKs from OpenAPI spec..."
	./scripts/generate-sdks.sh --all
	@echo "✅ All SDKs generated!"

check-types-drift: ## Check if TypeScript types are in sync with OpenAPI spec
	@./scripts/generate-sdks.sh --check-drift

determinism-check: ## Run determinism tests
	cargo test --test determinism_core_suite -- --test-threads=8 --test-timeout=45
	cargo test -p adapteros-lora-router --test determinism

# For faster runs: PROFILE=release make determinism-check
ifeq ($(PROFILE),release)
	cargo test --release --test determinism_core_suite -- --test-threads=8 --test-timeout=45
	cargo test --release -p adapteros-lora-router --test determinism
endif

e2e: ## Start stack, seed, run Cypress headless, then tear down
	@bash -c 'set -euo pipefail; trap "scripts/e2e/down.sh" EXIT; scripts/e2e/up.sh; scripts/e2e/seed.sh; pnpm --dir ui install --frozen-lockfile || pnpm --dir ui install; pnpm --dir ui exec cypress run --config-file ../cypress.config.ts'

KV_VERIFY_DB ?= ./var/aos-cp.sqlite3
KV_VERIFY_KV ?= ./var/aos-kv.redb
KV_VERIFY_DOMAINS ?= adapters,tenants,stacks,plans,auth_sessions,runtime_sessions,rag_artifacts,policy_audit,training_jobs,chat_sessions

# KV drift verification (CI-friendly)
kv-verify: ## Run SQL↔KV drift verification (fails on drift, no repair)
	mkdir -p $(dir $(KV_VERIFY_DB)) $(dir $(KV_VERIFY_KV))
	cargo run -p adapteros-cli -- db migrate --db-path $(KV_VERIFY_DB)
	cargo run -p adapteros-cli -- storage migrate --db-path $(KV_VERIFY_DB) --kv-path $(KV_VERIFY_KV) --domains $(KV_VERIFY_DOMAINS) --batch-size 200 --force
	@if [ -n "$(KV_VERIFY_OUT)" ]; then \
		cargo run -p adapteros-cli -- storage verify --json --db-path $(KV_VERIFY_DB) --kv-path $(KV_VERIFY_KV) --domains $(KV_VERIFY_DOMAINS) --fail-on-drift > $(KV_VERIFY_OUT); \
	else \
		cargo run -p adapteros-cli -- storage verify --json --db-path $(KV_VERIFY_DB) --kv-path $(KV_VERIFY_KV) --domains $(KV_VERIFY_DOMAINS) --fail-on-drift; \
	fi

# CI Integration: Add to test job after cargo test:
# make determinism-check || exit 1
# Use matrix: macos-13, macos-14 for cross-version verification

dup: ## Check for code duplication (fails on violations)
	bash scripts/run_jscpd.sh

ignored-tests-audit: ## Audit ignored tests: show mismatches between code and registry
	@echo "=== Ignored Tests Audit ==="
	@code_count=$$(grep -rn '#\[ignore *= *"' --include='*.rs' crates tests 2>/dev/null | grep -c 'tracking: STAB-IGN' || echo "0"); \
	echo "Tracked in code: $$code_count"; \
	echo "Run 'grep -rn \"#\[ignore\" --include=\"*.rs\" crates tests | grep -v tracking' to find untracked ignores"

ignored-tests-check: ## Strict check: fails if ignored tests lack tracking IDs
	@echo "=== Ignored Tests Check ==="
	@missing=$$(grep -rn '#\[ignore *= *"' --include='*.rs' crates tests 2>/dev/null | grep -v 'tracking: STAB-IGN' || true); \
	if [ -n "$$missing" ]; then \
		echo "FAIL: Found ignored tests without tracking IDs:"; \
		echo "$$missing" | head -10; \
		exit 1; \
	fi
	@echo "All ignored tests have tracking IDs"

# E2E worker startup harness (uses MLX 4-bit Qwen defaults from .env)
E2E_MODEL_PATH ?= ./var/models/Qwen2.5-7B-Instruct-4bit
E2E_BACKEND ?= mlx
E2E_UDS ?= ./var/run/aos-e2e.sock
E2E_MANIFEST ?= manifests/qwen7b-4bit-mlx.yaml
e2e-worker-test: ## Run aos-worker startup lifecycle test with MLX defaults
	AOS_E2E_MODEL_PATH=$(E2E_MODEL_PATH) \
	AOS_E2E_BACKEND=$(E2E_BACKEND) \
	AOS_E2E_UDS=$(E2E_UDS) \
	AOS_WORKER_MANIFEST=$(E2E_MANIFEST) \
	cargo test -p adapteros-lora-worker --test startup_lifecycle -- --nocapture

MLX_PACKAGE ?= adapteros-lora-mlx-ffi
MLX_FEATURES ?= multi-backend,mlx
MLX_PROFILE ?= release

verify-mlx-env: ## Verify MLX headers and libraries are available
	@if [ -z "$${MLX_INCLUDE_DIR}" ] || [ -z "$${MLX_LIB_DIR}" ]; then \
		echo "⚠️  MLX_INCLUDE_DIR or MLX_LIB_DIR not set. Set both to your MLX install (e.g., /opt/homebrew/include, /opt/homebrew/lib)."; \
	else \
		echo "✅ MLX_INCLUDE_DIR=$${MLX_INCLUDE_DIR}"; \
		echo "✅ MLX_LIB_DIR=$${MLX_LIB_DIR}"; \
		ls "$${MLX_INCLUDE_DIR}"/mlx >/dev/null 2>&1 && echo "✅ Found headers under $$MLX_INCLUDE_DIR/mlx" || echo "⚠️  Headers not found under $$MLX_INCLUDE_DIR/mlx"; \
		ls "$${MLX_LIB_DIR}"/libmlx.* >/dev/null 2>&1 && echo "✅ Found libmlx under $$MLX_LIB_DIR" || echo "⚠️  libmlx not found under $$MLX_LIB_DIR"; \
	fi

build-mlx: ## Build with real MLX backend (CoreML/Metal + MLX)
	$(MAKE) verify-mlx-env
	cargo build -p $(MLX_PACKAGE) --features $(MLX_FEATURES) --profile $(MLX_PROFILE)

test-mlx: verify-mlx-metal
	$(MAKE) verify-mlx-env
	cargo test -p $(MLX_PACKAGE) --features $(MLX_FEATURES)

bench-mlx: ## Run MLX benchmarks with real backend
	$(MAKE) verify-mlx-env
	cargo bench -p $(MLX_PACKAGE) --features $(MLX_FEATURES)

verify-mlx-metal: verify-mlx-env scripts/verify_metal_access.sh ## Verify MLX environment and Metal device access (required for GPU tests)

.DEFAULT_GOAL := help
