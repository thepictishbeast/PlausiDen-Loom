# PlausiDen-Loom Makefile — discovery + common operations.
#
# Per AVP-Doctrine rule docs-007 + [[tool-starvation-anti-pattern]]:
# `make help` is the cheapest, most-discoverable affordance for the
# tool surface. When Claude (or any contributor) lands in this repo,
# `make help` prints the canonical list.

.PHONY: help
help: ## Show this help.
	@printf '\n\033[1mPlausiDen-Loom — Makefile help\033[0m\n\n'
	@printf 'For the full surface see:\n'
	@printf '  AGENTS.md       — orientation for AI agents (read first)\n'
	@printf '  TOOLS.md        — canonical loom command index\n'
	@printf '  loom --help     — live CLI surface\n\n'
	@printf 'Common operations:\n\n'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z0-9_.-]+:.*?## / {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
	@printf '\n'

# ----------------------------------------------------------------
# Build + test (delegates to cargo)
# ----------------------------------------------------------------

.PHONY: build
build: ## Build the entire workspace (debug profile).
	cargo build --workspace

.PHONY: release
release: ## Build the workspace, release profile.
	cargo build --workspace --release

.PHONY: test
test: ## Run every workspace test.
	cargo test --workspace

.PHONY: clippy
clippy: ## Run clippy across the workspace (lint pass).
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format the workspace (rustfmt).
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Verify formatting without changing files (CI use).
	cargo fmt --all -- --check

# ----------------------------------------------------------------
# Loom CLI delegators
# ----------------------------------------------------------------

LOOM := ./target/release/loom

.PHONY: loom-cli
loom-cli: ## Build the release loom binary.
	cargo build --release -p loom-cli

.PHONY: emit-schema
emit-schema: loom-cli ## Regenerate cms-schema.json from loom-cms-render.
	cargo run -p loom-bridge -- emit-schema

# ----------------------------------------------------------------
# Maintenance + hygiene
# ----------------------------------------------------------------

.PHONY: clean
clean: ## Remove cargo build artifacts (target/).
	cargo clean

.PHONY: docs
docs: ## Generate workspace rustdoc (target/doc/).
	cargo doc --workspace --no-deps

.PHONY: ci
ci: fmt-check clippy test ## Run the CI gate set locally (fmt-check + clippy + test).
