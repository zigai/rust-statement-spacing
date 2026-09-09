use super::*;
use std::error::Error;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn raw_string_contents_are_unchanged() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    let text = r###\"one\n\n\n// not a comment\n}\"###;\n    use_text(text);\n}\n";
    let (fixed, _) = structural(source, &strict())?;
    assert!(fixed.contains("r###\"one\n\n\n// not a comment\n}\"###"));

    return Ok(());
}

#[test]
fn invalid_source_is_rejected_instead_of_recovery_editing() {
    let parse_error = parse_source("fn f( {", "2024").err();
    assert!(parse_error.is_some());
    assert_eq!(token_fingerprint("fn f( {", "2024").err(), parse_error);
}

#[test]
fn unsupported_edition_is_rejected() {
    assert!(parse_source("fn f() {}", "2050").is_err());
    assert!(token_fingerprint("fn f() {}", "2050").is_err());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn fingerprint_preserves_ordered_kind_names_and_token_bytes() -> Result<(), Box<dyn Error>> {
    let source = "m!(r#\"a  b\"#); // keep\n";
    let fingerprint = token_fingerprint(source, "2024")?;
    let tokens: Vec<_> = fingerprint
        .iter()
        .map(|(kind, text)| return (kind.as_str(), text.as_str()))
        .collect();
    assert_eq!(
        tokens,
        vec![
            ("IDENT", "m"),
            ("BANG", "!"),
            ("L_PAREN", "("),
            ("STRING", "r#\"a  b\"#"),
            ("R_PAREN", ")"),
            ("SEMICOLON", ";"),
            ("COMMENT", "// keep"),
        ]
    );
    assert_eq!(
        fingerprint,
        token_fingerprint("m! ( r#\"a  b\"# ) ;\n// keep\n", "2024")?
    );

    return Ok(());
}
