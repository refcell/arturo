# Arturo Justfile
# Minimal op-conductor rewrite using commonware ordered_broadcast

# Aliases
alias t := test
alias f := fix
alias b := build
alias c := clean

# Default recipe
default: check

# Run all CI checks
ci: fix check

# Run all checks (format, clippy, test, deny)
check: check-format check-clippy test check-deny

# Fix formatting and clippy issues
fix: format-fix clippy-fix

# Run tests
test:
    RUSTFLAGS="-D warnings" cargo nextest run --all-features

# Run tests with cargo test (fallback if nextest not installed)
test-cargo:
    RUSTFLAGS="-D warnings" cargo test --all-features

# Build in release mode
build:
    cargo build --release

# Build in debug mode
build-debug:
    cargo build

# Build with maximum performance profile
build-maxperf:
    cargo build --profile maxperf

# Clean build artifacts
clean:
    cargo clean

# Check formatting
check-format:
    cargo +nightly fmt --all -- --check

# Fix formatting issues
format-fix:
    cargo +nightly fmt --all

# Check clippy lints
check-clippy:
    RUSTFLAGS="-D warnings" cargo clippy --all-features --all-targets -- -D warnings

# Fix clippy issues
clippy-fix:
    cargo clippy --all-features --all-targets --fix --allow-dirty --allow-staged

# Check for unused dependencies
check-udeps:
    cargo +nightly udeps --all-features

# Check dependencies with cargo-deny
check-deny:
    cargo deny check

# Run documentation tests
doc-test:
    cargo test --doc --all-features

# Build documentation
doc:
    cargo doc --all-features --no-deps

# Watch for changes and run tests
watch-test:
    cargo watch -x 'nextest run --all-features'

# Watch for changes and run check
watch-check:
    cargo watch -x 'check --all-features'

# Run benchmarks (placeholder)
bench:
    @echo "No benchmarks configured yet"

# Show help
help:
    @just --list
