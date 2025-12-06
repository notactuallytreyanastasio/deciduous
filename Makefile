.PHONY: build release debug test test-verbose clean install uninstall serve analyze gen-test-files fmt lint check help db-nodes db-edges db-graph db-commands db-backup db-view goal decision option action outcome obs link status sync-graph deploy

# Default target
all: release

# Build targets
release:
	cargo build --release

debug:
	cargo build

build: release

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

BINARY := ./target/release/losselot

# View commands
db-nodes: release
	$(BINARY) db nodes

db-edges: release
	$(BINARY) db edges

db-graph: release
	$(BINARY) db graph

db-commands: release
	$(BINARY) db commands

db-backup: release
	$(BINARY) db backup

db-view: release
	@echo "Starting server and opening graph viewer..."
	$(BINARY) serve . --port $(or $(PORT),3001) &
	@sleep 1
	open http://localhost:$(or $(PORT),3001)/graph

# Create nodes (optional C=confidence 0-100)
goal: release
	@test -n "$(T)" || (echo "Usage: make goal T='Your goal title' [C=80]" && exit 1)
	$(BINARY) db add-node -t goal "$(T)" $(if $(C),-c $(C),)

decision: release
	@test -n "$(T)" || (echo "Usage: make decision T='Your decision title' [C=80]" && exit 1)
	$(BINARY) db add-node -t decision "$(T)" $(if $(C),-c $(C),)

option: release
	@test -n "$(T)" || (echo "Usage: make option T='Your option title' [C=80]" && exit 1)
	$(BINARY) db add-node -t option "$(T)" $(if $(C),-c $(C),)

action: release
	@test -n "$(T)" || (echo "Usage: make action T='Your action title' [C=80]" && exit 1)
	$(BINARY) db add-node -t action "$(T)" $(if $(C),-c $(C),)

outcome: release
	@test -n "$(T)" || (echo "Usage: make outcome T='Your outcome title' [C=80]" && exit 1)
	$(BINARY) db add-node -t outcome "$(T)" $(if $(C),-c $(C),)

obs: release
	@test -n "$(T)" || (echo "Usage: make obs T='Your observation' [C=80]" && exit 1)
	$(BINARY) db add-node -t observation "$(T)" $(if $(C),-c $(C),)

# Create edges
link: release
	@test -n "$(FROM)" || (echo "Usage: make link FROM=1 TO=2 [TYPE=leads_to] [REASON='why']" && exit 1)
	@test -n "$(TO)" || (echo "Usage: make link FROM=1 TO=2 [TYPE=leads_to] [REASON='why']" && exit 1)
ifdef REASON
	$(BINARY) db add-edge $(FROM) $(TO) -t $(or $(TYPE),leads_to) -r "$(REASON)"
else
	$(BINARY) db add-edge $(FROM) $(TO) -t $(or $(TYPE),leads_to)
endif

# Update status
status: release
	@test -n "$(ID)" || (echo "Usage: make status ID=1 S=completed" && exit 1)
	@test -n "$(S)" || (echo "Usage: make status ID=1 S=completed (pending|active|completed|rejected)" && exit 1)
	$(BINARY) db status $(ID) $(S)

# Help
help:
	@echo "Losselot - Audio Forensics Tool"
	@echo ""
	@echo "Build:"
	@echo "  make              Build release binary"
	@echo "  make release      Build release binary"
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
	@echo "Deploy:"
	@echo "  make sync-graph   Export decision graph to docs/demo/graph-data.json"
	@echo "  make deploy       Sync graph and push to main (triggers Pages build)"

# ============ Deploy ============

# Export decision graph to docs for GitHub Pages
sync-graph: release
	@echo "Exporting decision graph to docs/demo/graph-data.json..."
	$(BINARY) db graph > docs/demo/graph-data.json
	@echo "Graph exported: $$($(BINARY) db nodes | wc -l | tr -d ' ') nodes"

# Sync graph and push - triggers GitHub Pages deployment
deploy: sync-graph
	@echo "Decision graph synced. Ready to commit and push."
	@echo "Files changed:"
	@git status --short docs/demo/graph-data.json
