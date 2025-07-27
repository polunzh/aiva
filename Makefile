.PHONY: all build release test clean install uninstall run fmt lint check doc

# Variables
BINARY_NAME = aiva
INSTALL_PATH = /usr/local/bin
CARGO = cargo
RUST_LOG ?= info

# Default target
all: build

# Build debug version
build:
	@echo "Building AIVA..."
	$(CARGO) build

# Build release version
release:
	@echo "Building AIVA (release)..."
	$(CARGO) build --release

# Run tests
test:
	@echo "Running tests..."
	$(CARGO) test --workspace

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	$(CARGO) clean

# Install the binary
install: release
	@echo "Installing AIVA to $(INSTALL_PATH)..."
	@sudo cp target/release/$(BINARY_NAME) $(INSTALL_PATH)/
	@sudo chmod +x $(INSTALL_PATH)/$(BINARY_NAME)
	@echo "AIVA installed successfully!"

# Uninstall the binary
uninstall:
	@echo "Uninstalling AIVA..."
	@sudo rm -f $(INSTALL_PATH)/$(BINARY_NAME)
	@echo "AIVA uninstalled successfully!"

# Run the CLI
run:
	@RUST_LOG=$(RUST_LOG) $(CARGO) run --bin aiva -- $(ARGS)

# Format code
fmt:
	@echo "Formatting code..."
	$(CARGO) fmt --all

# Run linter
lint:
	@echo "Running clippy..."
	$(CARGO) clippy --workspace --all-targets -- -D warnings

# Run all checks (format, lint, test)
check: fmt lint test

# Generate documentation
doc:
	@echo "Generating documentation..."
	$(CARGO) doc --workspace --no-deps --open

# Development helpers
dev-init:
	@echo "Initializing development environment..."
	@mkdir -p ~/.aiva/{images,vms,logs}
	@echo "Development environment initialized!"

# Platform-specific builds
build-linux:
	@echo "Building for Linux..."
	$(CARGO) build --target x86_64-unknown-linux-gnu --release

build-macos:
	@echo "Building for macOS..."
	$(CARGO) build --target x86_64-apple-darwin --release

build-windows:
	@echo "Building for Windows..."
	$(CARGO) build --target x86_64-pc-windows-gnu --release

local-test:
	@aiva stop context7 || true
	@aiva delete context7 || true
	@limactl stop -f aiva-host
	@limactl delete aiva-host
	@aiva init context7 --template nodejs22-npx
	@aiva start context7
	@aiva run context7 "npx -y @upstash/context7-mcp --transport http --port 3005"
# Help
help:
	@echo "AIVA - Secure microVM environment for AI agents and MCP servers"
	@echo ""
	@echo "Usage:"
	@echo "  make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  all          - Build debug version (default)"
	@echo "  build        - Build debug version"
	@echo "  release      - Build release version"
	@echo "  test         - Run tests"
	@echo "  clean        - Clean build artifacts"
	@echo "  install      - Install AIVA to system"
	@echo "  uninstall    - Uninstall AIVA from system"
	@echo "  run          - Run AIVA (use ARGS=... to pass arguments)"
	@echo "  fmt          - Format code"
	@echo "  lint         - Run clippy linter"
	@echo "  check        - Run all checks (format, lint, test)"
	@echo "  doc          - Generate and open documentation"
	@echo "  dev-init     - Initialize development environment"
	@echo "  help         - Show this help message"
	@echo ""
	@echo "Examples:"
	@echo "  make build"
	@echo "  make run ARGS='init my-agent'"
	@echo "  make test"
	@echo "  make install"
