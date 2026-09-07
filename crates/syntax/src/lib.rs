//! Lossless syntax, comment ownership, and the source-to-compiler-facts join.
//!
//! This crate never guesses local identities from identifier spelling. In the
//! absence of compiler facts it can run structural checks only.

mod comments;
mod lower;
mod parse;
mod protection;
mod semantics;
mod shape;
mod units;

pub use parse::{ParsedSource, parse_source, token_fingerprint};
pub use semantics::{Anchor, Event, EventKind, SemanticIndex};
