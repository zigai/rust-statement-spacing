# rust-statement-spacing

[![crates.io](https://img.shields.io/crates/v/rust-statement-spacing.svg)](https://crates.io/crates/rust-statement-spacing)
[![Downloads](https://img.shields.io/crates/d/rust-statement-spacing.svg)](https://crates.io/crates/rust-statement-spacing)
[![docs.rs](https://docs.rs/rust-statement-spacing/badge.svg)](https://docs.rs/rust-statement-spacing)
![MSRV](https://img.shields.io/badge/MSRV-1.85.0-blue.svg)
[![License](https://img.shields.io/github/license/zigai/rust-statement-spacing.svg)](#license)

A configurable Rust linter for semantic blank-line placement and statement grouping, with autofixes.

## Installation

### As a library

```sh
cargo add rust-statement-spacing
```

## Usage

```rust
use rust_statement_spacing::package_name;

assert_eq!(package_name(), "rust-statement-spacing");
```

## Development

Install the required Rust components and fetch dependencies:

```sh
just setup
```

Run the built-in quality gate:

```sh
just check
```

Run `just help` to list formatting, linting, test, feature, coverage, documentation,
dependency-policy, spelling, and release recipes.

## Documentation

API documentation is available on [docs.rs](https://docs.rs/rust-statement-spacing).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
