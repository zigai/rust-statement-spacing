//! Source normalization and original-offset mapping.

use rust_statement_spacing_core::text::SourceText;

#[test]
fn normalization_maps_bom_unicode_and_crlf() {
    let raw = "\u{feff}Ž\r\nx\r\n".to_string();
    let text = SourceText::new(raw);
    assert_eq!(text.normalized, "Ž\nx\n");
    assert!(text.crlf);
    assert!(!text.mixed_newlines);
    assert_eq!(text.raw_offset(0), Some(3));
    assert_eq!(text.raw_offset(2), Some(5));
    assert_eq!(text.raw_offset(3), Some(7));
    assert_eq!(text.normalized_offset(6), None); // Removed LF half of CRLF.
    assert_eq!(text.original_newlines("\n\n    "), "\r\n\r\n    ");
}

#[test]
fn mixed_newlines_are_detected() {
    assert!(SourceText::new("a\r\nb\n".into()).mixed_newlines);
}
