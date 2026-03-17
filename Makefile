.DEFAULT_GOAL := help

# ── Colours ──────────────────────────────────────────────────────────────────
BOLD  := $(shell tput bold 2>/dev/null || echo '')
RESET := $(shell tput sgr0 2>/dev/null || echo '')
CYAN  := $(shell tput setaf 6 2>/dev/null || echo '')

# ── Targets ───────────────────────────────────────────────────────────────────

.PHONY: help
help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "$(CYAN)%-22s$(RESET) %s\n", $$1, $$2}'

.PHONY: build
build: ## Compile the library and binary (debug)
	cargo build

.PHONY: build-release
build-release: ## Compile with release optimisations
	cargo build --release

.PHONY: test
test: ## Run all tests (unit + integration + doctests)
	cargo test

.PHONY: test-unit
test-unit: ## Run unit tests only (lib crate, no integration tests)
	cargo test --lib

.PHONY: test-integration
test-integration: ## Run integration tests only
	cargo test --test rpc_integration

.PHONY: lint
lint: ## Run Clippy (deny warnings)
	cargo clippy -- -D warnings

.PHONY: fmt
fmt: ## Format all source files in-place
	cargo fmt

.PHONY: fmt-check
fmt-check: ## Check formatting without modifying files (CI)
	cargo fmt --check

.PHONY: check
check: fmt-check lint test ## Full pre-commit check: format + lint + test

.PHONY: run
run: ## Start the vault server with default settings
	cargo run

.PHONY: run-debug
run-debug: ## Start the vault server at debug log level
	cargo run -- --log-level debug

# ── OpenRPC ───────────────────────────────────────────────────────────────────

.PHONY: check-openrpc
check-openrpc: ## Validate openrpc.json structure and method coverage
	@./scripts/check_openrpc.sh

# ── CI convenience ────────────────────────────────────────────────────────────

.PHONY: ci
ci: fmt-check lint test check-openrpc ## Full CI pipeline (no network, no side effects)
