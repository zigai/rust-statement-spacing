use super::*;
use std::error::Error;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn trailing_comment_stays_with_preceding_statement() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    one(); // explanation\n    two();\n}\n";
    let (fixed, _) = structural(source, &strict())?;
    assert_eq!(
        fixed,
        "fn f() {\n    one(); // explanation\n\n    two();\n}\n"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn leading_comment_stays_with_following_statement() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    one();\n    // Explain the next action.\n    two();\n}\n";
    let (fixed, _) = structural(source, &strict())?;
    assert_eq!(
        fixed,
        "fn f() {\n    one();\n\n    // Explain the next action.\n    two();\n}\n"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn section_comment_is_preserved() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    one();\n\n    // Next phase\n\n    two();\n}\n";
    assert_eq!(structural(source, &strict())?.0, source);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn ambiguous_block_comment_is_preserved() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    one(); /* separate? */\n    two();\n}\n";
    assert_eq!(structural(source, &strict())?.0, source);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn function_doc_comment_and_attribute_remain_attached() -> Result<(), Box<dyn Error>> {
    let source = "fn first() {}\n/// Second function.\n#[inline]\nfn second() {}\n";
    let config = Config::default();
    let (fixed, _) = structural(source, &config)?;
    assert_eq!(
        fixed,
        "fn first() {}\n\n/// Second function.\n#[inline]\nfn second() {}\n"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn attribute_padding_is_removed_without_crossing_comments() -> Result<(), Box<dyn Error>> {
    let source = "#[inline]\n\n#[must_use]\n\nfn value() -> u8 { 1 }\n";
    let config = Config::only(&[Rule::Layout]);
    let expected = "#[inline]\n#[must_use]\nfn value() -> u8 { 1 }\n";
    assert_eq!(structural(source, &config)?.0, expected);
    assert!(structural(expected, &config)?.1.findings.is_empty());

    let commented = "#[inline]\n\n// This annotation is intentional.\nfn value() {}\n";
    assert_eq!(structural(commented, &config)?.0, commented);

    let protected = "#[rustfmt::skip]\n\nfn value() {}\n";
    assert_eq!(structural(protected, &config)?.0, protected);

    return Ok(());
}
