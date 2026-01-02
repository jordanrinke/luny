.PHONY: all build release test check clean install uninstall docs lint fmt help

# Default target
all: check test build

# Build debug version
build:
	cargo build

# Build optimized release version
release:
	cargo build --release

# Run all tests
test:
	cargo test

# Run tests with output
test-verbose:
	cargo test -- --nocapture

# Run checks (clippy + fmt check)
check:
	cargo fmt --check
	cargo clippy -- -D warnings

# Clean build artifacts
clean:
	cargo clean
	rm -rf .ai/

# Install to ~/.cargo/bin
install: release
	cargo install --path .

# Uninstall
uninstall:
	cargo uninstall luny

# Generate rustdoc documentation
docs:
	cargo doc --no-deps --open

# Run linter
lint:
	cargo clippy -- -D warnings

# Format code
fmt:
	cargo fmt

# Generate .toon files for this project
toon:
	cargo run --release -- generate src/ --force

# Validate .toon files
validate:
	cargo run --release -- validate --token-error 15000

# Publish to crates.io (dry run)
publish-dry:
	cargo publish --dry-run

# Publish to crates.io
publish:
	cargo publish

# Show help
help:
	@echo "Luny - Multi-language TOON DOSE generator"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  all          Run checks, tests, and build (default)"
	@echo "  build        Build debug version"
	@echo "  release      Build optimized release version"
	@echo "  test         Run all tests"
	@echo "  test-verbose Run tests with output"
	@echo "  check        Run fmt check and clippy"
	@echo "  clean        Remove build artifacts and .ai/"
	@echo "  install      Install to ~/.cargo/bin"
	@echo "  uninstall    Remove from ~/.cargo/bin"
	@echo "  docs         Generate and open rustdoc"
	@echo "  lint         Run clippy linter"
	@echo "  fmt          Format code with rustfmt"
	@echo "  toon         Generate .toon files for this project"
	@echo "  validate     Validate generated .toon files"
	@echo "  publish-dry  Dry run crates.io publish"
	@echo "  publish      Publish to crates.io"
	@echo "  help         Show this help"
