.PHONY: help build test clean fmt clippy metal ui ui-dev menu-bar menu-bar-dev menu-bar-install infra-check

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build all crates
	cargo build --release --locked --offline
	./scripts/build_metadata.sh
	./scripts/record_env.sh
	./scripts/strip_timestamps.sh

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

dup:
	bash scripts/run_jscpd.sh || true

.DEFAULT_GOAL := help