# Rust Statement Spacing

[![CI](https://github.com/zigai/rust-statement-spacing/actions/workflows/ci.yml/badge.svg)](https://github.com/zigai/rust-statement-spacing/actions/workflows/ci.yml)
[![rustc: 1.90+](https://img.shields.io/badge/rustc-1.90%2B-orange?logo=rust)](Cargo.toml)
[![Rust Edition: 2024](https://img.shields.io/badge/edition-2024-blue.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg)](#license)

A Rust linter and transactional autofixer for blank-line placement and statement grouping, inspired by [Go WSL](https://github.com/bombsimon/wsl). Keep related statements together and separate unrelated operations, control flow, and declarations with whitespace-only, compiler-verified fixes.

- **Context-aware statement grouping:** Groups statements using compiler-resolved bindings and dataflow relationships.
- **Comprehensive rules:** Checks bindings, expressions, control flow boundaries, early exits, `Result`/`Option` checks, and item declarations.
- **Safe transactional fixes:** Applies edits inside an isolated snapshot, verifying changes against `cargo fmt` and a secondary compiler run before touching disk.
- **Non-destructive:** Preserves comments, raw string literals, and macro interiors; rolls back cleanly if any check fails.

## Installation

```sh
cargo install --git https://github.com/zigai/rust-statement-spacing.git cargo-statement-spacing --locked
```

## Quick Start

### 1. Register the Dylint Library

Add the library to your workspace or project `Cargo.toml`:

```toml
[workspace.metadata.dylint]
libraries = [
    { git = "https://github.com/zigai/rust-statement-spacing", pattern = "lint" },
]
```

### 2. Run the Linter

```sh
cargo statement-spacing check
```


```sh
cargo statement-spacing fix --format-first
```

## Configuration

To customize rules or thresholds, add a `dylint.toml` to your workspace root:

```toml
[statement_spacing]
# Configuration schema version (must be 1).
schema_version = 1

# Optional extra rules to activate (e.g. ["layout"]).
enable = []

# Optional rules to deactivate (e.g. ["expressions"]).
disable = []
# When false, skips files containing "@generated" in their first 20 lines.
include_generated = false

# Workspace-relative glob patterns to ignore during linting.
exclude = ["target/**", "vendor/**"]


[statement_spacing.grouping]
# Policy for adjacent let bindings and assignments:
# - "consecutive": keep consecutive bindings together (default).
# - "same-kind":   join only bindings of the same syntactic kind.
# - "related":     join bindings with known context or dataflow relationships.
# - "preserve":    retain existing author spacing without enforcing changes.
bindings = "consecutive"

# Policy for adjacent ordinary expression statements:
# - "related":  group contextually related expressions together (default).
# - "strict":   enforce blank line separation between ordinary expressions.
# - "preserve": retain existing author spacing.
expressions = "related"

# Consider method calls sharing the same receiver object as related.
same_receiver = true

# Consider statements that read the same input variables as related.
shared_inputs = true

# Field projection matching on self:
# - "distinct": treat sibling fields (`self.a`, `self.b`) as distinct places (default).
# - "root":     relate all field accesses sharing the same root receiver.
self_fields = "distinct"

# Maximum setup statements permitted directly before control flow (0..1024).
max_before_control = 1

# Scope of variable reads considered when associating setup with control flow:
# - "header":                         condition or match expression only.
# - "header-or-first-body-statement": includes first direct statement in block body (default).
# - "whole-body":                     includes all non-deferred body reads.
use_in = "header-or-first-body-statement"

# How excess setup exceeding `max_before_control` is handled:
# - "related-suffix": retain only the bounded related suffix before control flow (default).
# - "whole-group":    separate the entire setup group from control flow.
overflow = "related-suffix"

# When true, automatically removes blank lines between related statements.
# When false, existing blank lines between related statements are preserved.
join_related = false


[statement_spacing.control_flow]
# Spacing following standalone blocks and control flow (if, match, loops):
# - "separate": require a separating blank line (default).
# - "preserve": retain existing spacing.
after_block = "separate"

# Spacing between consecutive short exiting guards (e.g. `if !ok { return; }`):
# - "allow":    permit guards to remain adjacent without blank lines (default).
# - "separate": require blank lines between guards.
guard_chain = "allow"


[statement_spacing.exits]
# Statement count threshold (0..1024) below which a block is considered short
# and exempt from mandatory tail separation.
short_block_max_statements = 2

# Spacing before final block value expressions / returns:
# - "smart":           considers block size and producer relationships (default).
# - "always-separate": always require a blank line before tail values.
# - "preserve":        retain existing tail spacing.
tail = "smart"


[statement_spacing.error_handling]
# Spacing between a call and immediate Result/Option check (?, unwrap, match):
# - "join":     keep producer and check adjacent (default).
# - "ordinary": apply standard expression grouping.
immediate_result_option_check = "join"


[statement_spacing.items]
# Spacing between function definitions with bodies:
# - "separate": require blank line separation (default).
# - "preserve": retain existing spacing.
functions = "separate"

# Spacing around major declarations (struct, enum, trait, impl):
# - "separate": require blank line separation (default).
# - "preserve": retain existing spacing.
major_items = "separate"

# Allow compact one-line declarations (type aliases, consts) to remain adjacent.
compact_declarations = true
```

---

## Lint Attributes

You can suppress or configure warnings at the crate, module, function, or block level using standard Rust attributes:

```rust
// Allow expression spacing checks for an entire module
#![allow(statement_spacing_expressions)]

// Suppress binding grouping on a specific function
#[allow(statement_spacing_bindings)]
fn setup_state() {
    let a = 1;
    let b = 2;
}
```

---

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
