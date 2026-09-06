//! Source-preserving vertical-spacing policies. There are no compiler dependencies here.
//!
//! All ranges refer to normalized UTF-8 source (LF newlines, no BOM). The compiler
//! adapter translates suggestions back to the original file's newline convention.

pub mod config;
pub mod edits;
mod model;
mod planner;
mod relations;
pub mod text;

pub use config::Config;
pub use edits::{Edit, apply_edits};
pub use model::{
    ByteRange, Facts, Gap, ItemKind, Place, Rule, RuleMask, SourceModel, Unit, UnitKind, UnitList,
};
pub use planner::{Finding, Plan, plan};
