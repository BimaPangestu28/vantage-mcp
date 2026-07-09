# vantage-mcp — developer tasks.
#
# `make` or `make help` lists targets. On Linux, a full build compiles screen
# capture (xcap) and OCR (Tesseract), which need system libraries — run
# `make setup-linux` once, or use the `*-nolibs` targets to skip them.

BIN := target/release/vantage-mcp

# Minimal MCP exchange used by the smoke targets: initialize, initialized,
# tools/list — one JSON-RPC message per line on stdin.
INIT := {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"make","version":"0"}}}
INITED := {"jsonrpc":"2.0","method":"notifications/initialized"}
TOOLS := {"jsonrpc":"2.0","id":2,"method":"tools/list"}

.DEFAULT_GOAL := help

.PHONY: help
help: ## List available targets
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

## --- Build ---------------------------------------------------------------

.PHONY: build
build: ## Release build (full: capture + OCR; needs Linux libs — see setup-linux)
	cargo build --release

.PHONY: build-nolibs
build-nolibs: ## Release build without capture/OCR system-lib requirements
	cargo build --release --no-default-features

.PHONY: debug
debug: ## Debug build of the whole workspace
	cargo build --workspace

## --- Test / lint ---------------------------------------------------------

.PHONY: test
test: ## Run all non-ignored tests (full features; needs Linux libs)
	cargo test --workspace

.PHONY: test-nolibs
test-nolibs: ## Run all non-ignored tests without capture/OCR libs
	cargo test --workspace --no-default-features

.PHONY: fmt
fmt: ## Format the workspace (rustfmt)
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Check formatting without modifying files
	cargo fmt --all --check

.PHONY: clippy
clippy: ## Lint with clippy (all targets)
	cargo clippy --workspace --all-targets

.PHONY: lint
lint: fmt-check clippy ## fmt-check + clippy

## --- Live tests (require a desktop session; #[ignore]d by default) --------

.PHONY: live-linux
live-linux: ## Run the ignored Linux live tests (AT-SPI, clipboard, capture, OCR, displays, act)
	cargo test -p vantage-platform-linux --test windows_live        -- --ignored
	cargo test -p vantage-platform-linux --test clipboard_live      -- --ignored
	cargo test -p vantage-platform-linux --test capture_live        -- --ignored
	cargo test -p vantage-platform-linux --test capture_window_live -- --ignored
	cargo test -p vantage-platform-linux --test displays_live       -- --ignored
	cargo test -p vantage-platform-linux --test ocr_live            -- --ignored
	cargo test -p vantage-platform-linux --test input_live          -- --ignored

.PHONY: live-linux-nolibs
live-linux-nolibs: ## Run only the Linux live tests that need no system libs (AT-SPI, clipboard, act)
	cargo test -p vantage-platform-linux --no-default-features --test windows_live   -- --ignored
	cargo test -p vantage-platform-linux --no-default-features --test clipboard_live -- --ignored
	cargo test -p vantage-platform-linux --no-default-features --test input_live     -- --ignored

.PHONY: live-macos
live-macos: ## Run the ignored macOS live tests
	cargo test -p vantage-platform-macos --test windows_live   -- --ignored
	cargo test -p vantage-platform-macos --test capture_live   -- --ignored
	cargo test -p vantage-platform-macos --test ocr_live       -- --ignored
	cargo test -p vantage-platform-macos --test clipboard_live -- --ignored

## --- Run -----------------------------------------------------------------

.PHONY: smoke
smoke: build ## Build then drive initialize + tools/list over stdio (prints JSON-RPC)
	@printf '%s\n' '$(INIT)' '$(INITED)' '$(TOOLS)' | $(BIN)

.PHONY: smoke-nolibs
smoke-nolibs: build-nolibs ## Same as smoke, using the lib-free binary
	@printf '%s\n' '$(INIT)' '$(INITED)' '$(TOOLS)' | $(BIN)

## --- Setup / housekeeping ------------------------------------------------

.PHONY: setup-linux
setup-linux: ## Install Linux system libs for capture (xcap) + OCR (Tesseract). Needs sudo in a real terminal.
	sudo apt-get update && sudo apt-get install -y \
		libwayland-dev libxcb1-dev libxcb-randr0-dev libxcb-shm0-dev \
		libdbus-1-dev libpipewire-0.3-dev libxkbcommon-dev \
		libegl-dev libgl-dev libgbm-dev \
		libtesseract-dev libleptonica-dev tesseract-ocr-eng clang libclang-dev pkg-config

.PHONY: check-linux-deps
check-linux-deps: ## Report whether the capture/OCR system libs are present
	@pkg-config --exists wayland-client && echo "wayland-client: OK" || echo "wayland-client: MISSING (capture)"
	@pkg-config --exists tesseract && echo "tesseract:      OK" || echo "tesseract:      MISSING (ocr)"
	@pkg-config --exists lept && echo "leptonica:      OK" || echo "leptonica:      MISSING (ocr)"
	@command -v clang >/dev/null && echo "clang:          OK" || echo "clang:          MISSING (ocr bindgen)"

.PHONY: clean
clean: ## Remove build artifacts
	cargo clean
