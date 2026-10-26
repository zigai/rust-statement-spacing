set positional-arguments

export RUSTDOCFLAGS := "-D warnings"

_:
  @just help

# Install required rustup components, fetch dependencies, and install pre-commit hooks
setup:
  rustup component add clippy rustfmt
  cargo fetch
  pre-commit install

# Format all Rust targets
format:
  cargo fmt --all
  cargo +stable fmt --manifest-path lint/Cargo.toml
  cargo +stable fmt --manifest-path fuzz/Cargo.toml

# Run Clippy with warnings treated as errors
lint:
  cargo clippy --workspace --all-targets --all-features -- -D warnings

# Apply safe Clippy fixes to tracked and staged work
fix:
  cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged

# Pass Cargo and libtest arguments after the recipe name, for example: just test -- --nocapture
# Run unit and integration tests with cargo-nextest
test *args:
  cargo nextest run --workspace --all-features {{args}}
# Run only rustdoc examples
doctest:
  cargo test --workspace --all-features --doc

# Check default, no-default, individual, and shallow feature combinations
features:
  cargo hack check --workspace --feature-powerset --depth 1 --all-targets

# Generate an HTML source coverage report
coverage:
  cargo llvm-cov --workspace --all-features --html

# Build a debug version
build:
  cargo build --workspace --all-features

# Build an optimized release version
release:
  cargo build --workspace --all-features --release

# Install the cargo-statement-spacing executable to ~/.cargo/bin
install:
  cargo install --locked --path crates/cli --force

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

# Remove Cargo build artifacts
clean:
  cargo clean

# Run the complete quality gate
check: lint test doctest test-driver static-check features docs-check deny spell
  cargo fmt --all -- --check
  cargo +stable fmt --manifest-path lint/Cargo.toml -- --check
  cargo +stable fmt --manifest-path fuzz/Cargo.toml -- --check
# List available commands
help:
  @just --list

alias fmt := format
alias cov := coverage

# Test the native transaction driver with explicit Cargo process doubles
test-driver:
  cargo build -p cargo-statement-spacing --locked
  python3 -m unittest discover -s crates/cli/tests -v

# Check source and configuration integrity
static-check:
  cargo run -p source-policy --locked


# Exercise the installed native consumer without relying on the source checkout
test-installed:
  python3 scripts/validate.py --portable-only

# Install the pinned Dylint toolchain and build the compiler plugin
setup-dylint:
  bash scripts/bootstrap.sh

# Exercise real Dylint checks and native/verified fixes
validate:
  python3 scripts/validate.py
