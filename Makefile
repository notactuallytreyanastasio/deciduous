.PHONY: build release test test-verbose clean install uninstall serve fmt lint check help precommit dialyzer credo

# Default target
all: build

# Build targets
build:
	mix compile

release:
	MIX_ENV=prod mix release --overwrite

escript:
	MIX_ENV=prod mix escript.build

# Testing
test:
	mix test

test-verbose:
	mix test --trace

test-filter:
	@test -n "$(FILTER)" || (echo "Usage: make test-filter FILTER=pattern" && exit 1)
	mix test --only $(FILTER)

# Code quality
fmt:
	mix format

fmt-check:
	mix format --check-formatted

lint: credo

credo:
	mix credo --strict

dialyzer:
	mix dialyzer

check: fmt-check credo dialyzer

precommit:
	mix precommit

# Installation
# - `make install` uses OTP release with CLI wrapper (no Zig required)
# - `make install-burrito` builds single binary with Burrito (requires Zig)
INSTALL_DIR := $(HOME)/.local/deciduex

install: release
	@echo "Installing deciduex to $(INSTALL_DIR)..."
	rm -rf $(INSTALL_DIR)
	mkdir -p $(INSTALL_DIR)
	cp -R _build/prod/rel/deciduex/* $(INSTALL_DIR)/
	mkdir -p $(HOME)/.local/bin
	ln -sf $(INSTALL_DIR)/bin/cli $(HOME)/.local/bin/deciduous
	@echo ""
	@echo "Installed:"
	@echo "  Release: $(INSTALL_DIR)/"
	@echo "  Binary:  $(HOME)/.local/bin/deciduous -> $(INSTALL_DIR)/bin/cli"

install-burrito:
	@command -v zig >/dev/null || (echo "Error: Zig not installed. Install with: brew install zig" && exit 1)
	BURRITO_TARGET=darwin_arm64 MIX_ENV=prod mix release --overwrite
	mkdir -p $(HOME)/.local/bin
	cp burrito_out/deciduex_darwin_arm64 $(HOME)/.local/bin/deciduous
	@echo "Installed single binary to $(HOME)/.local/bin/deciduous"

uninstall:
	rm -f $(HOME)/.local/bin/deciduous
	rm -rf $(INSTALL_DIR)
	@echo "Uninstalled deciduex"

# Clean build artifacts
clean:
	mix clean
	rm -rf _build deps

# Interactive server
serve:
	mix run -e 'Deciduex.Commands.Serve.run(%{port: 3000})'

# ============ Decision Graph ============

BINARY := mix run -e 'Deciduex.CLI.main(System.argv())' --

# View commands
db-nodes:
	$(BINARY) nodes

db-edges:
	$(BINARY) edges

db-graph:
	$(BINARY) graph

db-commands:
	$(BINARY) commands

db-backup:
	$(BINARY) backup

db-view:
	@echo "Starting server and opening graph viewer..."
	$(BINARY) serve --port $(or $(PORT),3001) &
	@sleep 2
	open http://localhost:$(or $(PORT),3001)

# Create nodes (optional C=confidence 0-100)
goal:
	@test -n "$(T)" || (echo "Usage: make goal T='Your goal title' [C=80]" && exit 1)
	$(BINARY) add goal "$(T)" $(if $(C),-c $(C),)

decision:
	@test -n "$(T)" || (echo "Usage: make decision T='Your decision title' [C=80]" && exit 1)
	$(BINARY) add decision "$(T)" $(if $(C),-c $(C),)

option:
	@test -n "$(T)" || (echo "Usage: make option T='Your option title' [C=80]" && exit 1)
	$(BINARY) add option "$(T)" $(if $(C),-c $(C),)

action:
	@test -n "$(T)" || (echo "Usage: make action T='Your action title' [C=80]" && exit 1)
	$(BINARY) add action "$(T)" $(if $(C),-c $(C),)

outcome:
	@test -n "$(T)" || (echo "Usage: make outcome T='Your outcome title' [C=80]" && exit 1)
	$(BINARY) add outcome "$(T)" $(if $(C),-c $(C),)

obs:
	@test -n "$(T)" || (echo "Usage: make obs T='Your observation' [C=80]" && exit 1)
	$(BINARY) add observation "$(T)" $(if $(C),-c $(C),)

# Create edges
link:
	@test -n "$(FROM)" || (echo "Usage: make link FROM=1 TO=2 [REASON='why']" && exit 1)
	@test -n "$(TO)" || (echo "Usage: make link FROM=1 TO=2 [REASON='why']" && exit 1)
ifdef REASON
	$(BINARY) link $(FROM) $(TO) -r "$(REASON)"
else
	$(BINARY) link $(FROM) $(TO)
endif

# Update status
status:
	@test -n "$(ID)" || (echo "Usage: make status ID=1 S=completed" && exit 1)
	@test -n "$(S)" || (echo "Usage: make status ID=1 S=completed" && exit 1)
	$(BINARY) status $(ID) $(S)

# Export graph
sync-graph:
	@echo "Exporting decision graph to docs/demo/graph-data.json..."
	$(BINARY) graph > docs/demo/graph-data.json
	@echo "Graph exported."

# ============ Cargo/Crates.io Release ============
#
# The Rust crate is a thin wrapper that embeds the Burrito binary.
# CI builds Burrito binaries, then cargo publish embeds them.

# Build Burrito binaries for all platforms (requires Zig)
burrito-all:
	@command -v zig >/dev/null || (echo "Error: Zig required. Install: brew install zig" && exit 1)
	BURRITO_TARGET=darwin_arm64 MIX_ENV=prod mix release --overwrite
	BURRITO_TARGET=darwin_amd64 MIX_ENV=prod mix release --overwrite
	BURRITO_TARGET=linux_amd64 MIX_ENV=prod mix release --overwrite
	@echo "Built Burrito binaries in burrito_out/"
	@ls -la burrito_out/

# Build Rust wrapper (requires Burrito binaries to exist)
cargo-build:
	cargo build --release

# Publish to crates.io (run burrito-all first!)
cargo-publish:
	@test -s burrito_out/deciduex_darwin_arm64 || (echo "Error: Run 'make burrito-all' first" && exit 1)
	cargo publish

cargo-publish-dry:
	@test -s burrito_out/deciduex_darwin_arm64 || (echo "Error: Run 'make burrito-all' first" && exit 1)
	cargo publish --dry-run

# Full release: build Burrito + publish Rust
release-cargo: burrito-all cargo-publish

# Help
help:
	@echo "Deciduex - Decision Graph Tooling (Elixir)"
	@echo ""
	@echo "Build:"
	@echo "  make              Compile project"
	@echo "  make build        Compile project"
	@echo "  make release      Build OTP release"
	@echo "  make escript      Build escript binary"
	@echo ""
	@echo "Test:"
	@echo "  make test         Run all tests"
	@echo "  make test-verbose Run tests with trace output"
	@echo "  make precommit    Run full quality checks"
	@echo ""
	@echo "Code Quality:"
	@echo "  make fmt          Format code"
	@echo "  make fmt-check    Check formatting"
	@echo "  make credo        Run Credo linter (strict)"
	@echo "  make dialyzer     Run Dialyzer type checker"
	@echo "  make check        Run all checks"
	@echo ""
	@echo "Install:"
	@echo "  make install      Install to ~/.local/bin"
	@echo "  make uninstall    Remove from ~/.local/bin"
	@echo ""
	@echo "Clean:"
	@echo "  make clean        Remove build artifacts"
	@echo ""
	@echo "Decision Graph:"
	@echo "  make db-nodes     List all decision nodes"
	@echo "  make db-edges     List all edges"
	@echo "  make db-graph     Show full graph as JSON"
	@echo "  make db-commands  Show recent command log"
	@echo "  make db-backup    Create database backup"
	@echo "  make db-view      Open graph viewer in browser"
	@echo ""
	@echo "  make goal T='...'      Add goal node"
	@echo "  make decision T='...'  Add decision node"
	@echo "  make option T='...'    Add option node"
	@echo "  make action T='...'    Add action node"
	@echo "  make outcome T='...'   Add outcome node"
	@echo "  make obs T='...'       Add observation node"
	@echo ""
	@echo "  make link FROM=1 TO=2 REASON='why'"
	@echo "  make status ID=1 S=completed"
	@echo ""
	@echo "  make sync-graph   Export graph to docs/"
