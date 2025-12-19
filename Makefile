.PHONY: build release release-full debug test test-verbose clean install uninstall serve analyze gen-test-files fmt lint check help db-nodes db-edges db-graph db-commands db-backup db-view goal decision option action outcome obs link status dot writeup sync-graph deploy publish publish-dry release-patch web-install web-dev web-build web-typecheck web-test web-preview trace-build trace-clear-cache

# Default target
all: release

# Detect macOS and set library path for libiconv if needed
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    export LIBRARY_PATH := /opt/homebrew/opt/libiconv/lib:$(LIBRARY_PATH)
endif

# Build targets
release:
	cargo build --release

debug:
	cargo build

build: release

# Build trace interceptor (tsc + esbuild bundle)
trace-build:
	cd trace-interceptor && npm install && npm run build && npm run bundle
	@echo "Trace interceptor built"

# Clear cached interceptor (forces re-extraction on next run)
trace-clear-cache:
	rm -rf ~/.deciduous/trace-interceptor
	@echo "Trace interceptor cache cleared"

# Full release build: trace interceptor + web viewer + Rust binary
release-full: trace-build web-build trace-clear-cache
	cp $(WEB_DIR)/dist/index.html src/viewer.html
	cp $(WEB_DIR)/dist/index.html docs/demo/index.html
	cargo build --release
	@echo "Full release build complete: target/release/deciduous"

# Testing
test:
	cargo test

test-verbose:
	cargo test -- --nocapture

test-filter:
	@test -n "$(FILTER)" || (echo "Usage: make test-filter FILTER=pattern" && exit 1)
	cargo test $(FILTER)

# Code quality
fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

lint:
	cargo clippy -- -D warnings

check:
	cargo check

# Run the analyzer
analyze:
	@test -n "$(FILE)" || (echo "Usage: make analyze FILE=path/to/file" && exit 1)
	cargo run --release -- $(FILE)

analyze-dir:
	@test -n "$(DIR)" || (echo "Usage: make analyze-dir DIR=path/to/directory" && exit 1)
	cargo run --release -- $(DIR)

analyze-no-spectral:
	@test -n "$(FILE)" || (echo "Usage: make analyze-no-spectral FILE=path/to/file" && exit 1)
	cargo run --release -- --no-spectral $(FILE)

# Generate reports
report-html:
	@test -n "$(FILE)" || (echo "Usage: make report-html FILE=path/to/file" && exit 1)
	cargo run --release -- -o report.html $(FILE)

report-json:
	@test -n "$(FILE)" || (echo "Usage: make report-json FILE=path/to/file" && exit 1)
	cargo run --release -- -o report.json $(FILE)

# Interactive web UI
serve:
	@echo "Starting web UI at http://localhost:3000"
	cargo run --release -- serve $(or $(DIR),.) --port $(or $(PORT),3000)

# Generate test files (requires ffmpeg, lame, sox)
gen-test-files:
	@command -v ffmpeg >/dev/null || (echo "Error: ffmpeg not found" && exit 1)
	@command -v lame >/dev/null || (echo "Error: lame not found" && exit 1)
	@command -v sox >/dev/null || (echo "Error: sox not found" && exit 1)
	./examples/generate_test_files.sh

# Analyze demo files
demo:
	@test -d examples/demo_files || (echo "Run 'make gen-test-files' first" && exit 1)
	cargo run --release -- examples/demo_files/

# Installation
install: release
	cp target/release/losselot /usr/local/bin/

uninstall:
	rm -f /usr/local/bin/losselot

# Clean build artifacts
clean:
	cargo clean

clean-reports:
	rm -f report.html report.json report.csv

clean-all: clean clean-reports

# Documentation
doc:
	cargo doc --open

# Development helpers
watch:
	@command -v cargo-watch >/dev/null || (echo "Install cargo-watch: cargo install cargo-watch" && exit 1)
	cargo watch -x test

bench:
	@test -n "$(FILE)" || (echo "Usage: make bench FILE=path/to/file" && exit 1)
	@echo "Timing analysis..."
	time cargo run --release -- $(FILE)

# ============ Decision Graph ============

BINARY := ./target/release/deciduous

# View commands
db-nodes: release
	$(BINARY) nodes

db-edges: release
	$(BINARY) edges

db-graph: release
	$(BINARY) graph

db-commands: release
	$(BINARY) commands

db-backup: release
	$(BINARY) backup

db-view: release
	@echo "Starting server and opening graph viewer..."
	$(BINARY) serve --port $(or $(PORT),3001) &
	@sleep 1
	open http://localhost:$(or $(PORT),3001)

# Create nodes (optional C=confidence 0-100)
goal: release
	@test -n "$(T)" || (echo "Usage: make goal T='Your goal title' [C=80]" && exit 1)
	$(BINARY) add goal "$(T)" $(if $(C),-c $(C),)

decision: release
	@test -n "$(T)" || (echo "Usage: make decision T='Your decision title' [C=80]" && exit 1)
	$(BINARY) add decision "$(T)" $(if $(C),-c $(C),)

option: release
	@test -n "$(T)" || (echo "Usage: make option T='Your option title' [C=80]" && exit 1)
	$(BINARY) add option "$(T)" $(if $(C),-c $(C),)

action: release
	@test -n "$(T)" || (echo "Usage: make action T='Your action title' [C=80]" && exit 1)
	$(BINARY) add action "$(T)" $(if $(C),-c $(C),)

outcome: release
	@test -n "$(T)" || (echo "Usage: make outcome T='Your outcome title' [C=80]" && exit 1)
	$(BINARY) add outcome "$(T)" $(if $(C),-c $(C),)

obs: release
	@test -n "$(T)" || (echo "Usage: make obs T='Your observation' [C=80]" && exit 1)
	$(BINARY) add observation "$(T)" $(if $(C),-c $(C),)

# Create edges
link: release
	@test -n "$(FROM)" || (echo "Usage: make link FROM=1 TO=2 [TYPE=leads_to] [REASON='why']" && exit 1)
	@test -n "$(TO)" || (echo "Usage: make link FROM=1 TO=2 [TYPE=leads_to] [REASON='why']" && exit 1)
ifdef REASON
	$(BINARY) link $(FROM) $(TO) -t $(or $(TYPE),leads_to) -r "$(REASON)"
else
	$(BINARY) link $(FROM) $(TO) -t $(or $(TYPE),leads_to)
endif

# Update status
status: release
	@test -n "$(ID)" || (echo "Usage: make status ID=1 S=completed" && exit 1)
	@test -n "$(S)" || (echo "Usage: make status ID=1 S=completed (pending|active|completed|rejected)" && exit 1)
	$(BINARY) status $(ID) $(S)

# DOT export
dot: release
	@if [ -n "$(AUTO)" ]; then \
		$(BINARY) dot --auto $(if $(NODES),--nodes "$(NODES)",) $(if $(ROOTS),--roots "$(ROOTS)",); \
	elif [ -n "$(NODES)" ]; then \
		if [ -n "$(PNG)" ]; then \
			$(BINARY) dot --nodes "$(NODES)" --png -o $(or $(OUT),decision-graph.dot); \
		else \
			$(BINARY) dot --nodes "$(NODES)" $(if $(OUT),-o $(OUT),); \
		fi \
	elif [ -n "$(ROOTS)" ]; then \
		if [ -n "$(PNG)" ]; then \
			$(BINARY) dot --roots "$(ROOTS)" --png -o $(or $(OUT),decision-graph.dot); \
		else \
			$(BINARY) dot --roots "$(ROOTS)" $(if $(OUT),-o $(OUT),); \
		fi \
	else \
		if [ -n "$(PNG)" ]; then \
			$(BINARY) dot --png -o $(or $(OUT),decision-graph.dot); \
		else \
			$(BINARY) dot $(if $(OUT),-o $(OUT),); \
		fi \
	fi

# PR writeup generation
writeup: release
	@if [ -n "$(AUTO)" ]; then \
		$(BINARY) writeup --auto $(if $(TITLE),-t "$(TITLE)",) $(if $(NODES),--nodes "$(NODES)",) $(if $(ROOTS),--roots "$(ROOTS)",) $(if $(OUT),-o $(OUT),); \
	elif [ -n "$(NODES)" ]; then \
		$(BINARY) writeup $(if $(TITLE),-t "$(TITLE)",) --nodes "$(NODES)" $(if $(PNG),--png "$(PNG)",) $(if $(OUT),-o $(OUT),); \
	elif [ -n "$(ROOTS)" ]; then \
		$(BINARY) writeup $(if $(TITLE),-t "$(TITLE)",) --roots "$(ROOTS)" $(if $(PNG),--png "$(PNG)",) $(if $(OUT),-o $(OUT),); \
	else \
		$(BINARY) writeup $(if $(TITLE),-t "$(TITLE)",) $(if $(PNG),--png "$(PNG)",) $(if $(OUT),-o $(OUT),); \
	fi

# Help
help:
	@echo "Deciduous - Decision Graph Tooling"
	@echo ""
	@echo "Build:"
	@echo "  make              Build release binary"
	@echo "  make release      Build release binary"
	@echo "  make release-full Build web viewer + release binary (full rebuild)"
	@echo "  make debug        Build debug binary"
	@echo ""
	@echo "Test:"
	@echo "  make test         Run all tests"
	@echo "  make test-verbose Run tests with output"
	@echo "  make test-filter FILTER=pattern  Run specific tests"
	@echo ""
	@echo "Code Quality:"
	@echo "  make fmt          Format code"
	@echo "  make fmt-check    Check formatting"
	@echo "  make lint         Run clippy linter"
	@echo "  make check        Quick compile check"
	@echo ""
	@echo "Analyze:"
	@echo "  make analyze FILE=path           Analyze a file"
	@echo "  make analyze-dir DIR=path        Analyze a directory"
	@echo "  make analyze-no-spectral FILE=path  Fast binary-only analysis"
	@echo "  make demo                        Analyze demo files"
	@echo ""
	@echo "Reports:"
	@echo "  make report-html FILE=path       Generate HTML report"
	@echo "  make report-json FILE=path       Generate JSON report"
	@echo ""
	@echo "Server:"
	@echo "  make serve                       Start web UI on port 3000"
	@echo "  make serve DIR=path PORT=8080    Custom directory and port"
	@echo ""
	@echo "Test Files:"
	@echo "  make gen-test-files              Generate test audio files"
	@echo "                                   (requires ffmpeg, lame, sox)"
	@echo ""
	@echo "Install:"
	@echo "  make install      Install to /usr/local/bin"
	@echo "  make uninstall    Remove from /usr/local/bin"
	@echo ""
	@echo "Clean:"
	@echo "  make clean        Remove build artifacts"
	@echo "  make clean-reports Remove generated reports"
	@echo "  make clean-all    Remove everything"
	@echo ""
	@echo "Dev:"
	@echo "  make watch        Auto-run tests on change (needs cargo-watch)"
	@echo "  make bench FILE=path  Time analysis of a file"
	@echo "  make doc          Build and open documentation"
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
	@echo "  make link FROM=1 TO=2           Link nodes"
	@echo "  make link FROM=1 TO=2 TYPE=chosen REASON='why'"
	@echo "  make status ID=1 S=completed    Update node status"
	@echo ""
	@echo "  make dot                        Export full graph as DOT"
	@echo "  make dot AUTO=1 NODES=1-11      Branch-specific filename (recommended!)"
	@echo "  make dot NODES=1-11 PNG=1       Export with PNG generation"
	@echo "  make dot ROOTS=1,5 PNG=1 OUT=graph.dot"
	@echo ""
	@echo "  make writeup                    Generate PR writeup"
	@echo "  make writeup AUTO=1 TITLE='PR' NODES=1-11  (recommended!)"
	@echo "  make writeup TITLE='PR' NODES=1-11 PNG=docs/graph.png"
	@echo "  make writeup ROOTS=1 OUT=PR.md"
	@echo ""
	@echo "Deploy & Publish:"
	@echo "  make sync-graph    Export decision graph to docs/demo/graph-data.json"
	@echo "  make deploy        Sync graph and push to main (triggers Pages build)"
	@echo "  make publish       Publish to crates.io (run tests first)"
	@echo "  make publish-dry   Dry-run publish to verify package"
	@echo "  make release-patch Bump patch version and publish"
	@echo ""
	@echo "Web Viewer (React + TypeScript + Vite):"
	@echo "  make web          Sync graph data and start dev server"
	@echo "  make web-dev      Start development server (http://localhost:3001)"
	@echo "  make web-build    Build production bundle"
	@echo "  make web-typecheck  Run TypeScript type checking"
	@echo "  make web-test     Run web tests"
	@echo "  make web-preview  Preview production build"
	@echo "  make web-sync     Sync graph data to web/public/"

# ============ Deploy & Publish ============

# Export decision graph to docs for GitHub Pages
sync-graph: release
	@echo "Exporting decision graph to docs/demo/graph-data.json..."
	$(BINARY) graph > docs/demo/graph-data.json
	@echo "Graph exported: $$($(BINARY) nodes | wc -l | tr -d ' ') nodes"

# Sync graph and push - triggers GitHub Pages deployment
deploy: sync-graph
	@echo "Decision graph synced. Ready to commit and push."
	@echo "Files changed:"
	@git status --short docs/demo/graph-data.json

# Publish to crates.io (bump version in Cargo.toml first)
publish: test
	@echo "Publishing to crates.io..."
	cargo publish

# Dry-run publish to verify package
publish-dry: test
	@echo "Dry-run publish to verify package..."
	cargo publish --dry-run

# Bump patch version and publish
release-patch:
	@echo "Bumping patch version..."
	@OLD_VERSION=$$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\([^"]*\)"/\1/'); \
	NEW_VERSION=$$(echo $$OLD_VERSION | awk -F. '{print $$1"."$$2"."$$3+1}'); \
	sed -i '' "s/version = \"$$OLD_VERSION\"/version = \"$$NEW_VERSION\"/" Cargo.toml; \
	echo "Version bumped: $$OLD_VERSION -> $$NEW_VERSION"; \
	git add Cargo.toml && git commit -m "Bump version to $$NEW_VERSION"; \
	cargo publish

# ============ Web Viewer (React + TypeScript + Vite) ============

WEB_DIR := web

# Install web dependencies
web-install:
	cd $(WEB_DIR) && npm install

# Start development server (hot reload)
web-dev: web-install
	@echo "Starting web viewer at http://localhost:3001"
	cd $(WEB_DIR) && npm run dev

# Build production bundle
web-build: web-install
	cd $(WEB_DIR) && npm run build

# TypeScript type checking
web-typecheck:
	cd $(WEB_DIR) && npm run typecheck

# Run web tests
web-test:
	cd $(WEB_DIR) && npm run test

# Preview production build
web-preview: web-build
	cd $(WEB_DIR) && npm run preview

# Sync graph data to web public folder (for dev)
web-sync: sync-graph
	cp docs/demo/graph-data.json $(WEB_DIR)/public/
	@echo "Graph data synced to web/public/"

# Full web development workflow
web: web-sync web-dev
