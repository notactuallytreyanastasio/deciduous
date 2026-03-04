# Deciduex Makefile - Elixir CLI for Deciduous
#
# Usage:
#   make build      - Build the release
#   make install    - Install to ~/bin/deciduous
#   make test       - Run tests
#   make precommit  - Full quality check
#   make clean      - Clean build artifacts

.PHONY: build install test precommit clean deps release

# Build directory
BUILD_DIR := _build/prod/rel/deciduex
INSTALL_DIR := $(HOME)/bin

# Default target
all: build

# Get dependencies
deps:
	mix deps.get

# Build release
build: deps
	MIX_ENV=prod mix release deciduex --overwrite
	@echo ""
	@echo "✅ Release built at $(BUILD_DIR)"
	@echo "   Run: $(BUILD_DIR)/bin/cli <command>"

# Build and install to ~/bin
install: build
	@mkdir -p $(INSTALL_DIR)
	@# Create a wrapper script that points to this release
	@echo '#!/bin/sh' > $(INSTALL_DIR)/deciduous
	@echo '# Deciduous CLI (Elixir) - beta install' >> $(INSTALL_DIR)/deciduous
	@echo 'exec "$(CURDIR)/$(BUILD_DIR)/bin/cli" "$$@"' >> $(INSTALL_DIR)/deciduous
	@chmod +x $(INSTALL_DIR)/deciduous
	@echo ""
	@echo "✅ Installed to $(INSTALL_DIR)/deciduous"
	@echo "   Make sure $(INSTALL_DIR) is in your PATH"
	@echo ""
	@echo "   Test with: deciduous --help"

# Uninstall
uninstall:
	rm -f $(INSTALL_DIR)/deciduous
	@echo "✅ Removed $(INSTALL_DIR)/deciduous"

# Run tests
test:
	mix test

# Full precommit check
precommit:
	mix precommit

# Run specific test file
test-file:
	mix test $(FILE)

# Interactive shell with release
console:
	$(BUILD_DIR)/bin/deciduex start_iex

# Clean build artifacts
clean:
	rm -rf _build
	rm -rf deps
	@echo "✅ Cleaned build artifacts"

# Development: compile only (faster)
compile:
	mix compile

# Format code
format:
	mix format

# Check formatting
format-check:
	mix format --check-formatted

# Run dialyzer
dialyzer:
	mix dialyzer

# Run credo
credo:
	mix credo --strict

# Quick build (dev mode, no release)
dev:
	mix compile
	@echo ""
	@echo "✅ Compiled. Run with:"
	@echo "   mix run -e 'Deciduex.CLI.main([\"nodes\"])'"

# Help
help:
	@echo "Deciduex Makefile"
	@echo ""
	@echo "Build targets:"
	@echo "  make build      - Build production release"
	@echo "  make install    - Build and install to ~/bin/deciduous"
	@echo "  make uninstall  - Remove from ~/bin"
	@echo "  make clean      - Clean build artifacts"
	@echo ""
	@echo "Test targets:"
	@echo "  make test       - Run all tests"
	@echo "  make precommit  - Full quality check (format, credo, dialyzer, tests)"
	@echo ""
	@echo "Development:"
	@echo "  make dev        - Quick compile (no release)"
	@echo "  make console    - Start IEx with release"
	@echo "  make format     - Format code"
	@echo "  make credo      - Run Credo linter"
	@echo "  make dialyzer   - Run Dialyzer"
