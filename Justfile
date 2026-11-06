set positional-arguments

export RUSTDOCFLAGS := "-D warnings"

# Install repository dependencies
setup:
    @rustup component add clippy rustfmt
    @cargo fetch

# Format every supported first-party file type
format:
    @ruff format .
    @cargo statement-spacing fix --format-first
    @cargo +stable fmt --manifest-path lint/Cargo.toml
    @cargo +stable fmt --manifest-path fuzz/Cargo.toml
    @oxfmt '!**/*.md' '!.github/dependabot.yml'
    @git ls-files -co --exclude-standard -z | while IFS= read -r -d '' file; do if [[ -f $file ]]; then printf '%s\0' "$file"; fi; done | xargs -0 -r shfmt -f | xargs -r -d '\n' shfmt -w
    @git ls-files -co --exclude-standard -- Justfile justfile "*.just" | while IFS= read -r file; do just --justfile "$file" --fmt; done
    @just markdown fmt

# Run every configured linter on the whole project, stopping on failure
lint:
    @cargo clippy --workspace --all-targets --all-features -- -D warnings

# Apply safe lint autofixes, then format every supported file type
fix: && format
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged

# Run fast Rust unit tests
[positional-arguments]
test *args:
    @cargo nextest run --workspace --all-features "$@"

# Run only rustdoc examples
doctest:
    cargo test --workspace --all-features --doc

# Check default, no-default, individual, and shallow feature combinations
features:
    cargo hack check --workspace --feature-powerset --depth 1 --all-targets

# Measure all local test suites and print coverage
coverage:
    #!/usr/bin/env bash
    set -euo pipefail
    export CARGO_TARGET_DIR="$PWD/target"
    cargo llvm-cov clean --workspace
    cargo llvm-cov --no-report nextest --workspace --all-features --ignore-default-filter
    eval "$(cargo llvm-cov show-env --sh)"
    cargo test --workspace --all-features --doc
    cargo llvm-cov report --package '*' --all-features

# Build a debug version
build:
    cargo build --workspace --all-features

# Install the cargo-statement-spacing executable to ~/.cargo/bin
install: && (report-install env("CARGO_INSTALL_ROOT", env("CARGO_HOME", home_directory() / ".cargo")) / "bin/cargo-statement-spacing")
    cargo install --locked --path crates/cli --force

# Build API documentation and reject rustdoc warnings
docs-check:
    cargo doc --workspace --all-features --no-deps

# Build API documentation
docs: docs-check

# Check dependency advisories, licenses, bans, and sources
deny:
    cargo deny check

# Remove generated output and caches
clean: (remove "coverage" "lcov.info" "*.profraw" "validation-output" ".ruff_cache" "lint/target" "fuzz/target" "**/__pycache__" "target/llvm-cov-target")
    @cargo clean

# Run the commit checks
check: (run-checks "formatcheck" "lockcheck" "static-check" "lint" "features" "docs-check" "deny" "test" "doctest" "test-driver")

# Run the commit checks and slow native verification
checkfull: (run-checks "formatcheck" "lockcheck" "static-check" "lint" "features" "docs-check" "deny" "test" "doctest" "test-driver" "validate" "test-installed")

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

# Check the same files and tools without writing
formatcheck:
    @ruff format --check .
    @cargo fmt --check
    @cargo statement-spacing check
    @cargo +stable fmt --check --manifest-path lint/Cargo.toml
    @cargo +stable fmt --check --manifest-path fuzz/Cargo.toml
    @oxfmt --check '!**/*.md' '!.github/dependabot.yml'
    @git ls-files -co --exclude-standard -z | while IFS= read -r -d '' file; do if [[ -f $file ]]; then printf '%s\0' "$file"; fi; done | xargs -0 -r shfmt -f | xargs -r -d '\n' shfmt -d
    @git ls-files -co --exclude-standard -- Justfile justfile "*.just" | while IFS= read -r file; do just --justfile "$file" --fmt --check; done
    @just markdown check

# Synchronize dependency locks without upgrading dependencies
[no-exit-message]
lock:
    @cargo update --workspace
    @cd "lint" && cargo update --workspace
    @cd "tests/workspaces/basic" && cargo update --workspace

# Verify dependency manifests and locks without changing files
[no-exit-message]
lockcheck:
    @cargo metadata --locked --format-version 1 > /dev/null
    @cd "lint" && cargo metadata --locked --format-version 1 > /dev/null
    @cd "tests/workspaces/basic" && cargo metadata --locked --format-version 1 > /dev/null

# List available recipes
[default]
help:
    @just --list

alias fmt := format
alias cov := coverage

[positional-arguments]
[private]
remove +paths:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob globstar
    size() { LC_ALL=C numfmt --from-unit=1024 --to=iec-i --suffix=B --format=%.1f "$1"; }
    rows=() total=0
    for pattern in "$@"; do
        found=()
        for path in $pattern; do
            if [[ -e $path || -L $path ]]; then found+=("$path"); fi
        done
        if (( ${#found[@]} == 0 )); then continue; fi
        kib=$(du -sck -- "${found[@]}" | tail -n1 | cut -f1)
        rm -rf -- "${found[@]}"
        label=$pattern
        if (( ${#found[@]} > 1 )); then label+=" (${#found[@]})"; fi
        rows+=("removed  $label|$(size "$kib")")
        total=$(( total + kib ))
    done
    if [[ -n ${CLEAN_TOTAL_FILE:-} ]]; then echo "$total" >> "$CLEAN_TOTAL_FILE"; fi
    if (( ${#rows[@]} == 0 )); then echo "nothing to clean"; exit 0; fi
    printf '%s\n' "${rows[@]}" "total freed|$(size "$total")" | column -t -s '|' -R 2

[positional-arguments]
[private]
report-install +paths:
    #!/usr/bin/env bash
    set -euo pipefail
    rows=() warnings=() status=0
    for path in "$@"; do
        name=$(basename "$path") real=$(realpath "$path")
        sized=$real venv=$(dirname "$(dirname "$real")")
        [[ -f $venv/pyvenv.cfg ]] && sized=$venv
        if ! version=$("$path" --version 2>&1); then
            "$path" --help >/dev/null 2>&1 || { echo "failed     $path does not start" >&2; status=1; continue; }
            version=-
        fi
        rows+=("installed|$name|${path/#$HOME/\~}|$(du -sk "$sized" | cut -f1 | LC_ALL=C numfmt --from-unit=1024 --to=iec-i --suffix=B --format=%.1f)|${version%%$'\n'*}")
        first=$(command -v "$name")
        [[ $(realpath "$first") == "$real" ]] || warnings+=("warning    $name: PATH finds ${first/#$HOME/\~} first")
    done
    if (( ${#rows[@]} )); then printf '%s\n' "${rows[@]}" | column -t -s '|' -R 4; fi
    if (( ${#warnings[@]} )); then printf '%s\n' "${warnings[@]}"; fi
    exit "$status"

[positional-arguments]
[private]
run-checks +steps:
    #!/usr/bin/env bash
    set -uo pipefail
    LC_NUMERIC=C
    log_dir="${XDG_CACHE_HOME:-$HOME/.cache}/just-check"
    mkdir -p "$log_dir"
    log=$(mktemp "$log_dir/step.XXXXXX")
    trap 'rm -f "$log"' EXIT
    width=0 failed=()
    for step in "$@"; do (( ${#step} > width )) && width=${#step}; done
    for step in "$@"; do
        read -ra command <<< "$step"
        printf 'check  %-*s  ' "$width" "$step"
        start=$EPOCHREALTIME
        run=(just --justfile {{ quote(justfile()) }} "${command[@]}")
        if [[ -t 1 ]]; then run=(script -qefc "${run[*]@Q}" /dev/null); fi
        if "${run[@]}" < /dev/null > "$log" 2>&1; then status=ok; else status=FAIL; failed+=("$step"); fi
        printf '%-4s  %7.1fs\n' "$status" "$(LC_NUMERIC=C awk -v a="$start" -v b="$EPOCHREALTIME" 'BEGIN { print b - a }')"
        if [[ "$status" == FAIL ]]; then sed 's/\r$//; s/^/    /' "$log"; fi
    done
    if (( ${#failed[@]} )); then printf '%d of %d failed: %s\n' "${#failed[@]}" "$#" "${failed[*]}"; exit 1; fi
    printf 'all %d passed\n' "$#"

[private]
markdown *args:
    rumdl {{ args }} --disable MD001,MD013,MD024,MD025,MD026,MD031,MD032,MD033,MD034,MD036,MD041,MD058 --config 'MD060.enabled=true' --config 'MD060.style="aligned"' --config 'MD003.style="atx"' --config 'MD004.style="dash"' --config 'MD029.style="ordered"' --config 'MD046.style="fenced"' --config 'MD048.style="backtick"' --config 'MD049.style="asterisk"' --config 'MD050.style="asterisk"' --config 'MD055.style="leading_and_trailing"'
