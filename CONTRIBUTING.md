# Development

## Environment

Install Rust with [rustup](https://rustup.rs/). This repository uses Rust 2024,
declares Rust 1.85.0 as its minimum supported Rust version (MSRV), and uses
`stable` for normal development.

[`just`](https://github.com/casey/just) is the documented command runner. Cargo commands remain
available directly when `just` is not installed.

## Initial setup

```sh
just setup
```

This installs `rustfmt` and Clippy and fetches Cargo dependencies. Install the optional repository
tools with:

```sh
just install-tools
```

That recipe installs cargo-nextest, cargo-llvm-cov, cargo-deny, typos-cli, cargo-hack, and cargo-semver-checks.

## Quality checks

Run the built-in quality gate:

```sh
just check
```

It verifies formatting, compilation of all targets and features, Clippy with warnings denied, tests,
doctests where applicable, and rustdoc warnings.

Run the broader tool-assisted gate after installing the optional tools:

```sh
just check-all
```

### Formatting

```sh
just format
```

### Clippy

```sh
just lint
```

Lint suppressions should be narrow and include a reason. Prefer fixing the design or configuring a
specific lint over broad crate-level allowances.

### Testing

```sh
just test
just nextest
```

`cargo test` runs documentation tests by default. cargo-nextest does not, so `just check-all` runs
`just doctest` separately for projects with a library target.

The generated test workflow, when selected, runs on Linux, macOS, and Windows and separately verifies
the declared MSRV.

### Feature combinations

```sh
just features
```

The cargo-hack recipe checks default, no-default, individual, and shallow feature combinations. Keep
features additive; expand the matrix deliberately when integrations interact.

### Coverage

```sh
just coverage
```

The HTML report is written below `target/llvm-cov/`.

### Dependency policy

```sh
just deny
```

Update `deny.toml` deliberately when introducing a new license, registry, Git dependency, or an
unavoidable duplicate dependency version.

### Spelling

```sh
just spell
```

## Pre-commit hooks

Install [pre-commit](https://pre-commit.com/) and then run:

```sh
just hooks
```

The configured hooks check general file hygiene, Rust formatting, and Clippy.

## Lockfile policy

This is a library-only crate, so `Cargo.lock` is ignored. CI resolves dependencies from the declared
version requirements.

## Unsafe Rust

The workspace forbids unsafe Rust. Changing that policy is an architectural decision and must include
a documented safety boundary, invariants, targeted tests, and appropriate dynamic checking.

## Releases

Before releasing:

```sh
just check-all
just package-list
just package
just publish-dry-run
```

Update `CHANGELOG.md`, set the package version in `Cargo.toml`, and create a matching GitHub release
tag such as `v0.2.0`.

Publish locally with `cargo publish` after reviewing the packaged file list.

Check public API compatibility against the latest published release with:

```sh
just semver
```
