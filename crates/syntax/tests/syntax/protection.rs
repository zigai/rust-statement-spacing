use super::*;
use std::error::Error;

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

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn derive_helpers_do_not_protect_but_skip_still_does() -> Result<(), Box<dyn Error>> {
    let helper = "use uuid::Uuid;\n/// Doc.\n#[derive(Clone, Debug)]\n#[serde(rename_all = \"snake_case\")]\nenum E {\n    A,\n}\n";
    let spaced = "use uuid::Uuid;\n\n/// Doc.\n#[derive(Clone, Debug)]\n#[serde(rename_all = \"snake_case\")]\nenum E {\n    A,\n}\n";
    assert_eq!(structural(helper, &strict())?.0, spaced);
    let skipped = "#[derive(Clone)]\n#[rustfmt::skip]\nfn f() {\n    one();two();\n}\n";
    assert_eq!(structural(skipped, &strict())?.0, skipped);
    let transformed = "#[derive(Clone)]\n#[some_transform]\nfn f() {\n    one();\n    two();\n}\n";
    assert_eq!(structural(transformed, &strict())?.0, transformed);

    return Ok(());
}
