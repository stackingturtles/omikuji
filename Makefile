.PHONY: build run test clean fmt lint check doc release install help tag docker docker-build docker-run

# Default target
all: build

# Build the project
build:
	cargo build

# Run the project with default config
run:
	cargo run

# Run with custom config file
run-config:
	cargo run -- -c config.yaml

# Run all tests
test:
	cargo test

# Run tests with output
test-verbose:
	cargo test -- --nocapture

# Test database setup
test-db-setup:
	@./scripts/setup-test-db.sh

# Run tests with test database
test-with-db: test-db-setup
	@DATABASE_URL="postgresql://omikuji:omikuji_password@localhost:5433/omikuji_test" cargo test

# Run specific test with database
test-one-with-db: test-db-setup
	@DATABASE_URL="postgresql://omikuji:omikuji_password@localhost:5433/omikuji_test" cargo test $(TEST)

# Run tests using the test runner script
test-db:
	@./scripts/run-tests.sh

# Regenerate SQLx query cache for offline builds
sqlx-prepare:
	DATABASE_URL="postgresql://omikuji:omikuji_password@localhost:5433/omikuji_db" cargo sqlx prepare
	@echo "SQLx query cache updated. Remember to commit .sqlx/ directory"

# Clean build artifacts
clean:
	cargo clean

# Format code
fmt:
	cargo fmt

# Check code formatting
fmt-check:
	cargo fmt --check

# Run clippy linter
lint:
	cargo clippy

# Run clippy with warnings as errors
lint-strict:
	cargo clippy -- -D warnings

# Run clippy with GitHub Actions CI settings (includes uninlined_format_args)
lint-ci:
	cargo clippy -- -D warnings -D clippy::uninlined_format_args

# Run full CI-style linting (format check + clippy with CI settings)
ci-lint:
	@./scripts/lint.sh

# Fix linting issues automatically where possible
lint-fix:
	@echo "Fixing code formatting..."
	@cargo fmt
	@echo "Fixing clippy issues..."
	@cargo clippy --fix --allow-dirty -- -D warnings -D clippy::uninlined_format_args
	@echo "Done! Run 'make ci-lint' to check remaining issues."

# Run all checks (format, lint, test)
check: fmt-check lint test

# Run exact CI checks (matches GitHub Actions)
ci-check:
	@echo "Running CI checks locally..."
	cargo fmt -- --check
	cargo clippy -- -D warnings
	cargo test --verbose
	cargo build --verbose

# Build optimized release version
release:
	SQLX_OFFLINE=true cargo build --release

# Run release version
run-release:
	cargo run --release

# Generate documentation
doc:
	cargo doc --open

# Install the binary locally
install:
	cargo install --path .

# Update dependencies
update:
	cargo update

# Check for outdated dependencies
outdated:
	cargo outdated

# Run security audit
audit:
	cargo audit

# Generate code coverage report (requires grcov)
coverage:
	@echo "Cleaning previous coverage data..."
	@rm -f *.profraw
	@cargo clean
	@echo "Building with coverage instrumentation..."
	@CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo build
	@echo "Running tests with coverage..."
	@CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test
	@echo "Generating coverage report..."
	@grcov . --binary-path ./target/debug/deps/ -s . -t html --branch --ignore-not-existing --ignore '../*' --ignore "/*" -o ./target/coverage
	@echo "Coverage report generated at ./target/coverage/index.html"

# Generate coverage report in lcov format
coverage-lcov:
	@echo "Cleaning previous coverage data..."
	@rm -f *.profraw
	@cargo clean
	@echo "Building with coverage instrumentation..."
	@CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo build
	@echo "Running tests with coverage..."
	@CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test
	@echo "Generating lcov coverage report..."
	@grcov . --binary-path ./target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../*' --ignore "/*" -o coverage.lcov
	@echo "Coverage report generated at coverage.lcov"

# Install coverage tools
install-coverage-tools:
	@echo "Installing grcov..."
	@cargo install grcov
	@echo "Installing llvm-tools..."
	@rustup component add llvm-tools-preview
	@echo "Coverage tools installed!"

# Development mode with auto-reload (requires cargo-watch)
watch:
	cargo watch -x run

# Run with debug logging
debug:
	RUST_LOG=debug cargo run

# Run with trace logging
trace:
	RUST_LOG=trace cargo run

# Tag a new release
tag:
ifndef VERSION
	$(error VERSION is not set. Usage: make tag VERSION=x.y.z)
endif
	@echo "Updating version to $(VERSION) in Cargo.toml..."
	@# Update version in Cargo.toml (works on both macOS and Linux)
	@if [ "$$(uname)" = "Darwin" ]; then \
		sed -i '' 's/^version = ".*"/version = "$(VERSION)"/' Cargo.toml; \
	else \
		sed -i 's/^version = ".*"/version = "$(VERSION)"/' Cargo.toml; \
	fi
	@echo "Running cargo check to update Cargo.lock..."
	@cargo check --quiet
	@echo "Committing version changes..."
	@git add Cargo.toml Cargo.lock
	@git commit -m "chore: bump version to $(VERSION)" || echo "No changes to commit"
	@echo "Creating tag v$(VERSION)..."
	@git tag -a v$(VERSION) -m "Release v$(VERSION)"
	@echo "Version updated and tag created!"
	@echo "Push with: git push && git push origin v$(VERSION)"

# Build Docker image locally
docker-build:
	docker build -t omikuji:latest .

# Run Docker container
docker-run:
	docker run -v $(PWD)/config.yaml:/config/config.yaml omikuji:latest

# Build multi-platform Docker image
docker-buildx:
	docker buildx build --platform linux/amd64,linux/arm64 -t omikuji:latest .

# Help target
help:
	@echo "Available targets:"
	@echo "  make build        - Build the project"
	@echo "  make run          - Run with default config"
	@echo "  make run-config   - Run with config.yaml"
	@echo "  make test         - Run all tests"
	@echo "  make test-verbose - Run tests with output"
	@echo "  make test-db      - Run tests with fresh test database"
	@echo "  make test-with-db - Run all tests with test database"
	@echo "  make test-one-with-db TEST=name - Run specific test with database"
	@echo "  make sqlx-prepare - Regenerate SQLx query cache for offline builds"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make fmt          - Format code"
	@echo "  make fmt-check    - Check code formatting"
	@echo "  make lint         - Run clippy linter"
	@echo "  make lint-strict  - Run clippy with strict warnings"
	@echo "  make lint-ci      - Run clippy with GitHub Actions CI settings"
	@echo "  make ci-lint      - Run full CI-style linting checks"
	@echo "  make lint-fix     - Fix linting issues automatically"
	@echo "  make check        - Run all checks (format, lint, test)"
	@echo "  make ci-check     - Run exact CI pipeline locally"
	@echo "  make release      - Build optimized release"
	@echo "  make run-release  - Run release version"
	@echo "  make doc          - Generate documentation"
	@echo "  make install      - Install binary locally"
	@echo "  make update       - Update dependencies"
	@echo "  make outdated     - Check for outdated dependencies"
	@echo "  make audit        - Run security audit"
	@echo "  make coverage     - Generate HTML code coverage report"
	@echo "  make coverage-lcov - Generate LCOV code coverage report"
	@echo "  make install-coverage-tools - Install grcov and llvm-tools"
	@echo "  make watch        - Run with auto-reload (needs cargo-watch)"
	@echo "  make debug        - Run with debug logging"
	@echo "  make trace        - Run with trace logging"
	@echo "  make tag VERSION=x.y.z - Tag a new release"
	@echo "  make docker-build - Build Docker image locally"
	@echo "  make docker-run   - Run Docker container"
	@echo "  make docker-buildx - Build multi-platform Docker image"
	@echo "  make help         - Show this help message"