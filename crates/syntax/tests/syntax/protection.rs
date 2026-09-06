use std::error::Error;

use super::*;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn macro_token_interiors_are_opaque() -> Result<(), Box<dyn Error>> {
    let source = "macro_rules! rules { ($x:tt) => {\n    one();\n    two();\n}; }\n";
    assert_eq!(structural(source, &strict())?.0, source);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn rustfmt_skip_protects_entire_subtree() -> Result<(), Box<dyn Error>> {
    let source = "#[rustfmt::skip]\nfn f() {\n    one();\n    two();\n}\n";
    assert_eq!(structural(source, &strict())?.0, source);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn conditional_rustfmt_skip_is_conservative() -> Result<(), Box<dyn Error>> {
    let source =
        "#[cfg_attr(feature = \"x\", rustfmt::skip)]\nfn f() {\n    one();\n    two();\n}\n";
    assert_eq!(structural(source, &strict())?.0, source);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn unknown_procedural_attribute_is_protected() -> Result<(), Box<dyn Error>> {
    let source = "#[some_transform]\nfn f() {\n    one();\n    two();\n}\n";
    assert_eq!(structural(source, &strict())?.0, source);
    return Ok(());
}
