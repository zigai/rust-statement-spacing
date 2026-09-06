set positional-arguments

export RUSTDOCFLAGS := "-D warnings"

_:
  @just help

# Install required rustup components and fetch dependencies
setup:
  rustup component add clippy rustfmt
  cargo fetch

# Make a current stable toolchain available for compiling repository tools
_stable-tools:
  rustup toolchain install stable --profile minimal

# Install cargo-nextest
install-nextest: _stable-tools
  cargo +stable install --locked cargo-nextest

# Install cargo-llvm-cov and its rustup component
install-coverage-tool: _stable-tools
  rustup component add llvm-tools-preview
  cargo +stable install --locked cargo-llvm-cov

# Install cargo-deny
install-deny-tool: _stable-tools
  cargo +stable install --locked cargo-deny

# Install typos-cli
install-spell-tool: _stable-tools
  cargo +stable install --locked typos-cli

# Install cargo-hack
install-feature-tool: _stable-tools
  cargo +stable install --locked cargo-hack

# Install cargo-semver-checks
install-semver-tool: _stable-tools
  cargo +stable install --locked cargo-semver-checks

# Install all optional repository tools
install-tools: install-nextest install-coverage-tool install-deny-tool install-spell-tool install-feature-tool install-semver-tool

# Format all Rust targets
format:
  cargo fmt --all

# Compile every workspace target and feature
check-code:
  cargo check --workspace --all-targets --all-features

# Run Clippy with warnings treated as errors
lint:
  cargo clippy --workspace --all-targets --all-features -- -D warnings

# Apply safe Clippy fixes to tracked and staged work
fix:
  cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged

# Pass Cargo and libtest arguments after the recipe name, for example: just test -- --nocapture
# Run unit, integration, and documentation tests
test *args:
  cargo test --workspace --all-features {{args}}

# Nextest does not run documentation tests; check-all runs them separately for library crates.
# Run unit and integration tests with cargo-nextest
nextest *args:
  cargo nextest run --workspace --all-features {{args}}

# Run only rustdoc examples
doctest:
  cargo test --workspace --all-features --doc

# Check default, no-default, individual, and shallow feature combinations
features:
  cargo hack check --workspace --feature-powerset --depth 1 --no-dev-deps --all-targets

# Generate an HTML source coverage report
coverage:
  cargo llvm-cov --workspace --all-features --html

# Generate an LCOV coverage file
coverage-lcov:
  cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info

# Build a debug version
build:
  cargo build --workspace --all-features

# Build an optimized release version
release:
  cargo build --workspace --all-features --release

# Build API documentation and reject rustdoc warnings
docs-check:
  cargo doc --workspace --all-features --no-deps

# Build API documentation
docs: docs-check

# Check dependency advisories, licenses, bans, and sources
deny:
  cargo deny check

# Check prose and identifiers for common misspellings
spell:
  typos

# Check public API compatibility with the latest published release
semver:
  cargo semver-checks check-release --package rust-statement-spacing

# List the files that would be included in the crate archive
package-list:
  cargo package --package rust-statement-spacing --list

# Create and verify the distributable crate archive
package:
  cargo package --package rust-statement-spacing

# Perform all publication checks without uploading
publish-dry-run:
  cargo publish --package rust-statement-spacing --dry-run

# Install the repository's pre-commit hooks
hooks:
  pre-commit install

# Remove Cargo build artifacts
clean:
  cargo clean

# Run the built-in non-mutating quality gate
check: check-code lint test docs-check
  cargo fmt --all -- --check

# Run the broader tool-assisted quality gate
check-all: check-code
  cargo fmt --all -- --check lint nextest doctest features docs-check deny spell

# List available commands
help:
  @just --list

alias fmt := format
alias cov := coverage
alias dev := setup
alias qa := check
alias test-fast := nextest

