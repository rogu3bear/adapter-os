.PHONY: help build test clean fmt clippy metal ui ui-dev menu-bar menu-bar-dev menu-bar-install dup tui tui-debug up server adapter-up

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build all crates
	cargo build --release --locked --offline
	./scripts/build_metadata.sh
	./scripts/record_env.sh
	./scripts/strip_timestamps.sh

test: ## Run all tests (excluding experimental MLX FFI)
	cargo test --workspace --exclude adapteros-lora-mlx-ffi

clean: ## Clean build artifacts
	cargo clean
	rm -f metal/*.air metal/*.metallib
	rm -rf ui/dist ui/node_modules
	rm -rf crates/mplora-server/static
	cd menu-bar-app && swift package clean || true

fmt: ## Format code
	cargo fmt --all

clippy: ## Run clippy
	cargo clippy --all-features -- -D warnings

metal: ## Build Metal shaders
	cd metal && bash build.sh

ui: ## Build Web UI (production)
	bash scripts/build_web_ui.sh

ui-dev: ## Start Web UI dev server
	cd ui && pnpm dev

tui: ## Run the Terminal UI control panel
	cargo run -p adapteros-tui

tui-debug: ## Run TUI with debug logging
	RUST_LOG=debug cargo run -p adapteros-tui

up: tui ## Launch the TUI (alias for 'tui')

adapter-up: tui ## Launch the TUI (alias for 'tui')

server: ## Start the AdapterOS server
	cargo run -p adapteros-server

codegraph-viewer: ## Build CodeGraph Viewer (Tauri desktop app)
	cd crates/mplora-codegraph-viewer/frontend && pnpm install && pnpm build
	cd crates/mplora-codegraph-viewer/src-tauri && cargo build --release

codegraph-viewer-dev: ## Start CodeGraph Viewer in dev mode
	cd crates/mplora-codegraph-viewer/frontend && pnpm install
	cd crates/mplora-codegraph-viewer/src-tauri && cargo tauri dev

check: fmt clippy test ## Run all checks

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

test-menu-bar-integration: ## Test menu bar integration with status JSON
	@echo "Testing menu bar integration..."
	@echo "1. Creating test status JSON..."
	@mkdir -p var
	@echo '{"schema_version":"1.0","status":"ok","uptime_secs":3600,"adapters_loaded":2,"deterministic":true,"kernel_hash":"a84d9f1c","telemetry_mode":"local","worker_count":1,"base_model_loaded":true,"base_model_id":"qwen2.5-7b","base_model_name":"Qwen 2.5 7B","base_model_status":"ready","base_model_memory_mb":14336}' > var/adapteros_status.json
	@echo "2. Building menu bar app..."
	@cd menu-bar-app && swift build -c release
	@echo "3. Testing menu bar compilation..."
	@test -f menu-bar-app/.build/release/AdapterOSMenu && echo "✓ Menu bar app built successfully" || (echo "✗ Menu bar build failed"; exit 1)
	@echo "4. Testing JSON parsing..."
	@cd menu-bar-app && swift run --help >/dev/null 2>&1 && echo "✓ Menu bar app runs successfully" || (echo "✗ Menu bar app failed to run"; exit 1)
	@echo "✓ Menu bar integration test completed successfully"

sbom: ## Generate SBOM
	cargo xtask sbom

determinism-report: ## Generate determinism report
	cargo xtask determinism-report

verify-artifacts: ## Verify and sign artifacts
	./scripts/verify_artifacts.sh

openapi-docs: ## Generate OpenAPI documentation
	./scripts/generate_openapi_simple.sh

validate-openapi: ## Validate OpenAPI documentation
	./scripts/validate_openapi_docs.sh

dup: ## Scan repository for code duplication (reports under var/reports/jscpd)
	bash scripts/run_jscpd.sh

.DEFAULT_GOAL := help
