# clamor common tasks. Run `just` to list, `just check` for the full gate.

# List available recipes.
default:
    @just --list

# Format with nightly rustfmt.
fmt:
    cargo +nightly fmt --all

# Check formatting (CI gate).
fmt-check:
    cargo +nightly fmt --all --check

# Lint with clippy, warnings as errors.
clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Run the test suite.
test:
    cargo test --workspace --all-features --locked

# Build the workspace.
build:
    cargo build --workspace

# Full build/lint/test gate (matches CI and the README).
check: fmt-check clippy test

# Install the clamor binary from this checkout into the cargo bin path.
install:
    cargo install --path clamor --locked
