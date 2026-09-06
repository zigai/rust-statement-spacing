//! Behavioral regression tests for spacing syntax and semantic joins.

use rust_statement_spacing_core::config::Expressions;
use rust_statement_spacing_core::*;
use rust_statement_spacing_syntax::{
    Event, EventKind, SemanticIndex, parse_source, token_fingerprint,
};

fn structural(source: &str, config: &Config) -> Result<(String, Plan), String> {
    let mut parsed = parse_source(source, "2024")?;
    let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
    parsed.attach(source, &anchors);
    let result = plan(config, source, &parsed.model)?;
    let fixed = apply_edits(source, &result.edits())?;
    if token_fingerprint(source, "2024")? != token_fingerprint(&fixed, "2024")? {
        return Err("token fingerprint mismatch after edits".into());
    }
    return Ok((fixed, result));
}

fn strict() -> Config {
    let mut config = Config::default();
    config.grouping.expressions = Expressions::Strict;
    return config;
}

mod syntax {
    use super::*;

    mod comments;
    mod parsing;
    mod protection;
    mod semantics;
    mod spacing;
    mod units;
}
