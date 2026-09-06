#![no_main]

use libfuzzer_sys::fuzz_target;
use rust_statement_spacing_core::{Config, apply_edits, plan};
use rust_statement_spacing_syntax::{SemanticIndex, parse_source, token_fingerprint};

fuzz_target!(|input: &str| {
    if input.len() > 32_768 {
        return;
    }
    let Ok(mut parsed) = parse_source(input, "2024") else {
        return;
    };
    let semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
    parsed.attach(input, &semantics);
    let first = plan(&Config::default(), input, &parsed.model).expect("valid parsed model");
    let fixed = apply_edits(input, &first.edits()).expect("safe whitespace edits");
    assert_eq!(
        token_fingerprint(input, "2024").unwrap(),
        token_fingerprint(&fixed, "2024").unwrap()
    );
    let mut parsed = parse_source(&fixed, "2024").expect("fixes preserve syntax");
    let semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
    parsed.attach(&fixed, &semantics);
    let second = plan(&Config::default(), &fixed, &parsed.model).expect("idempotent model");
    assert!(second.edits().is_empty());
});
