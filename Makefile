.PHONY: help build prepare test clean fmt clippy metal ui ui-dev menu-bar menu-bar-dev menu-bar-install infra-check dev dev-no-auth build-mlx test-mlx bench-mlx verify-mlx-env

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

dev: ## Run control plane in dev mode (auth bypass available, no prod hardening). Set NO_AUTH=1 to disable auth middleware.
	AOS_DEV_NO_AUTH=$(NO_AUTH) cargo run -p adapteros-server -- --config configs/cp.toml --skip_pf_check

dev-no-auth: ## Run control plane in dev mode with authentication disabled (debug-only)
	NO_AUTH=1 $(MAKE) dev

prepare: ## Prepare build environment: stop services, clean ports
	@echo "🧹 Preparing build environment (stopping services, cleaning ports)..."
	./scripts/fresh-build.sh
	@echo "✅ Build environment ready!"

download-model: ## Download Qwen 2.5 7B Instruct model (~3.8GB)
	@./scripts/download-model.sh

check-system: ## Check system readiness before launch (preflight checks)
	@./scripts/check-system.sh

test: ## Run all tests (excluding experimental MLX FFI)
	cargo test --workspace --exclude adapteros-lora-mlx-ffi
	cargo miri test --lib adapteros_lora_worker

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

sbom: ## Generate Software Bill of Materials
	@mkdir -p var/security
	@echo "Generating SBOM..."
	@cargo tree > var/security/sbom-$(shell date +%Y%m%d-%H%M%S).txt
	@echo "✅ SBOM generated in var/security/"

license-check: ## Check dependency license compliance
	@cargo install cargo-license --quiet 2>/dev/null || true
	@cargo license --json > var/security/licenses-$(shell date +%Y%m%d-%H%M%S).json
	@echo "✅ License report generated in var/security/"

fmt: ## Format code
	cargo fmt --all

clippy: ## Run clippy
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

check: fmt clippy test determinism-check ## Run all checks

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

determinism-check: ## Run determinism tests
	cargo test --test determinism_harness -- --test-threads=8 --test-timeout=45
	cargo test -p adapteros-lora-router --test determinism

# For faster runs: PROFILE=release make determinism-check
ifeq ($(PROFILE),release)
	cargo test --release --test determinism_harness -- --test-threads=8 --test-timeout=45
	cargo test --release -p adapteros-lora-router --test determinism
endif

# CI Integration: Add to test job after cargo test:
# make determinism-check || exit 1
# Use matrix: macos-13, macos-14 for cross-version verification

dup: ## Check for code duplication (fails on violations)
	bash scripts/run_jscpd.sh

MLX_PACKAGE ?= adapteros-lora-mlx-ffi
MLX_FEATURES ?= multi-backend,real-mlx
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

test-mlx: ## Run MLX unit and integration tests with real backend
	$(MAKE) verify-mlx-env
	cargo test -p $(MLX_PACKAGE) --features $(MLX_FEATURES)

bench-mlx: ## Run MLX benchmarks with real backend
	$(MAKE) verify-mlx-env
	cargo bench -p $(MLX_PACKAGE) --features $(MLX_FEATURES)

.DEFAULT_GOAL := help
