#!/usr/bin/env bash
# Installs explicitly pinned development tools into the invoking user's Rust setup.
# Does not change any consumer project's Rust files. Requires rustup + Python 3.11+.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
command -v rustup >/dev/null || { echo 'Install rustup first.' >&2; exit 1; }
python3 -c 'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ is required"'
rustup toolchain install 1.96.0 --profile minimal --component rustfmt --component clippy
rustup toolchain install nightly-2026-05-28 --profile minimal \
    --component rustc-dev --component llvm-tools-preview --component rustfmt
# CI caches can restore binaries without Cargo's installation tracking metadata.
cargo +nightly-2026-05-28 install cargo-dylint dylint-link --version '=6.0.4' --locked --force
(
    cd "$ROOT"
    cargo +1.96.0 generate-lockfile
)
(
    # Correct cwd matters: Cargo must load lint/.cargo/config.toml for dylint-link.
    cd "$ROOT/lint"
    cargo +nightly-2026-05-28 generate-lockfile
    cargo +nightly-2026-05-28 build --release --locked
)
echo 'Dependency locks resolved. Review and retain both Cargo.lock files.'
echo "Run: cd '$ROOT' && python3 scripts/validate.py"
