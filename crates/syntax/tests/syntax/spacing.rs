use std::error::Error;

use super::*;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn basic_statement_separation() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    one();\n    two();\n}\n";
    let (fixed, result) = structural(source, &strict())?;
    assert_eq!(result.edits().len(), 1);
    assert_eq!(fixed, "fn f() {\n    one();\n\n    two();\n}\n");
    assert!(structural(&fixed, &strict())?.1.findings.is_empty());
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn single_line_boundary_has_diagnostic_without_fix() -> Result<(), Box<dyn Error>> {
    let source = "fn f() { one(); two(); }\n";
    let (fixed, result) = structural(source, &strict())?;
    assert_eq!(fixed, source);
    assert!(!result.findings.is_empty());
    assert!(result.edits().is_empty());
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn layout_removes_only_whitespace_padding() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n\n    one();\n\n}\n";
    let config = Config::only(&[Rule::Layout]);
    assert_eq!(structural(source, &config)?.0, "fn f() {\n    one();\n}\n");
    return Ok(());
}
