//! Whitespace edit safety and conflict rejection.

use rust_statement_spacing_core::edits::blank_line_replacement;
use rust_statement_spacing_core::{ByteRange, Edit, Rule, apply_edits};

#[test]
fn replacement_preserves_indentation_not_comments() {
    assert_eq!(blank_line_replacement("\n\n\t", 0), Some("\n\t".into()));
    assert_eq!(blank_line_replacement("\n    ", 1), Some("\n\n    ".into()));
    assert!(blank_line_replacement(" ", 1).is_none());
    assert!(blank_line_replacement("\n// comment\n", 1).is_none());
}

#[test]
fn non_trivia_and_stale_edits_are_rejected() {
    let mut edit = Edit {
        range: ByteRange::new(1, 2),
        expected: " ".into(),
        replacement: "\n".into(),
        rule: Rule::Bindings,
    };
    assert!(apply_edits("a b", &[edit.clone()]).is_ok());
    edit.replacement = "unsafe{}".into();
    assert!(apply_edits("a b", &[edit.clone()]).is_err());
    edit.replacement = "\n".into();
    assert!(apply_edits("axb", &[edit]).is_err());
}

#[test]
fn utf8_split_is_rejected() {
    let edit = Edit {
        range: ByteRange::new(1, 2),
        expected: "".into(),
        replacement: "\n".into(),
        rule: Rule::Bindings,
    };
    assert!(apply_edits("Ž", &[edit]).is_err());
}

#[test]
fn same_position_edits_are_rejected() {
    let edit = Edit {
        range: ByteRange::new(1, 1),
        expected: "".into(),
        replacement: "\n".into(),
        rule: Rule::Bindings,
    };
    assert!(apply_edits("ab", &[edit.clone(), edit]).is_err());
}
