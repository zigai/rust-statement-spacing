use super::*;
use rust_statement_spacing_core::config::{GuardChain, Tail};
use std::error::Error;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn nested_functions_are_not_outer_statement_groups() -> Result<(), Box<dyn Error>> {
    let source =
        "fn outer() {\n    fn inner() {\n        one();\n        two();\n    }\n    inner();\n}\n";
    let (fixed, _) = structural(source, &strict())?;
    assert!(fixed.contains("one();\n\n        two();"));
    assert!(fixed.contains("}\n\n    inner();"));

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn if_else_and_let_else_are_single_source_units() -> Result<(), Box<dyn Error>> {
    let source = "fn f(x: Option<u8>) {\n    let Some(x) = x else {\n        return;\n    };\n    if x == 0 {\n        one();\n    } else {\n        two();\n    }\n}\n";
    let parsed = parse_source(source, "2024")?;
    let let_start = source.find("let Some").ok_or("expected let-else fixture")?;
    let outer = parsed
        .model
        .lists
        .iter()
        .find(|list| {
            return list
                .units
                .first()
                .is_some_and(|unit| return unit.code_range.start == let_start);
        })
        .ok_or("expected outer function body list")?;
    assert_eq!(outer.units.len(), 2);
    assert_eq!(outer.units[0].kind, UnitKind::Let);
    assert_eq!(outer.units[1].kind, UnitKind::Control);
    assert_eq!(
        source.get(outer.units[0].range.as_range()),
        Some("let Some(x) = x else {\n        return;\n    };")
    );
    assert_eq!(
        source.get(outer.units[1].range.as_range()),
        Some("if x == 0 {\n        one();\n    } else {\n        two();\n    }")
    );
    // Only the outer gap is constrained here; nested bodies own their own gaps.
    assert_eq!(outer.gaps.len(), 1);
    assert_eq!(
        outer.gaps[0].range,
        ByteRange::new(outer.units[0].range.end, outer.units[1].range.start)
    );

    structural(source, &strict())?;

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn let_match_is_a_binding_not_an_after_block_unit() -> Result<(), Box<dyn Error>> {
    let source = "fn f(x: Option<u8>) {\n    let y = match x {\n        Some(y) => y,\n        None => 0,\n    };\n    use_y(y);\n}\n";
    let config = Config::only(&[Rule::AfterBlock]);
    assert_eq!(structural(source, &config)?.0, source);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn match_arm_boundary_is_not_a_statement_boundary() -> Result<(), Box<dyn Error>> {
    let source =
        "fn f(x: u8) {\n    match x {\n        0 => one(),\n        _ => two(),\n    }\n}\n";
    assert_eq!(structural(source, &strict())?.0, source);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn out_of_line_modules_remain_compact() -> Result<(), Box<dyn Error>> {
    let source = "mod first;\nmod second;\npub(super) mod third;\npub(super) mod fourth;\n";
    let (fixed, result) = structural(source, &Config::default())?;
    assert_eq!(fixed, source);
    assert!(result.findings.is_empty());

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn compact_declarations_separate_families_not_individual_items() -> Result<(), Box<dyn Error>> {
    let source = "use std::io;\nuse std::fs;\nconst FIRST: u8 = 1;\nstatic SECOND: u8 = 2;\ntype Value = u8;\ntype Other = u16;\n";
    let expected = "use std::io;\nuse std::fs;\n\nconst FIRST: u8 = 1;\nstatic SECOND: u8 = 2;\n\ntype Value = u8;\ntype Other = u16;\n";
    let mut config = Config::default();
    assert_eq!(structural(source, &config)?.0, expected);
    assert!(structural(expected, &config)?.1.findings.is_empty());

    config.disable.push(Rule::ItemSpacing);
    assert_eq!(structural(source, &config)?.0, source);

    config.disable.clear();
    config.items.compact_declarations = false;
    let separated = "use std::io;\n\nuse std::fs;\n\nconst FIRST: u8 = 1;\n\nstatic SECOND: u8 = 2;\n\ntype Value = u8;\n\ntype Other = u16;\n";
    assert_eq!(structural(source, &config)?.0, separated);

    return Ok(());
}

#[test]
fn import_origins_do_not_split_a_contiguous_group() {
    let source = "use crate::support::*;\n\nuse std::env;\n\npub use external::{First, Second};\n";
    let expected = "use crate::support::*;\nuse std::env;\npub use external::{First, Second};\n";
    let mut config = Config::default();
    config.grouping.join_related = true;
    assert_eq!(
        structural(source, &config).map(|(fixed, _)| return fixed),
        Ok(expected.to_owned())
    );
    assert_eq!(
        structural(expected, &config).map(|(fixed, _)| return fixed),
        Ok(expected.to_owned())
    );

    config.grouping.join_related = false;
    assert_eq!(
        structural(source, &config).map(|(fixed, _)| return fixed),
        Ok(source.to_owned())
    );
}

#[test]
fn normalization_joins_declaration_families_across_visibility_and_layout() {
    let source = concat!(
        "use std::io;\n\npub use core::{\n    fmt,\n    mem,\n};\n",
        "use external::First;\n\npub(crate) use external::{\n    Second,\n};\n",
        "mod first;\n\npub mod second;\n\npub(crate) mod third;\n\npub(super) mod fourth;\n",
        "type First = u8;\n\npub type Second = u16;\n",
        "const FIRST: u8 = 1;\n\npub static SECOND: u8 = 2;\n",
    );
    let expected = concat!(
        "use std::io;\npub use core::{\n    fmt,\n    mem,\n};\n",
        "use external::First;\npub(crate) use external::{\n    Second,\n};\n\n",
        "mod first;\npub mod second;\npub(crate) mod third;\npub(super) mod fourth;\n\n",
        "type First = u8;\npub type Second = u16;\n\n",
        "const FIRST: u8 = 1;\npub static SECOND: u8 = 2;\n",
    );
    let mut config = Config::default();
    config.grouping.join_related = true;
    assert_eq!(
        structural(source, &config).map(|(fixed, _)| return fixed),
        Ok(expected.to_owned())
    );
    assert_eq!(
        structural(expected, &config).map(|(fixed, _)| return fixed),
        Ok(expected.to_owned())
    );
}

#[test]
fn declaration_normalization_preserves_attached_attributes_and_comment_sections() {
    let source = concat!(
        "use std::io;\n",
        "// External declarations.\n#[allow(\n    unused_imports,\n\n    dead_code\n)]\n",
        "use external::Thing;\n",
        "/// Module documentation.\n#[path = \"other.rs\"]\nmod other;\n",
        "/// Alias documentation.\n#[allow(dead_code)]\ntype Value = u8;\n",
        "\n// Another alias section.\n\ntype Other = u16;\n",
    );
    let expected = concat!(
        "use std::io;\n",
        "// External declarations.\n#[allow(\n    unused_imports,\n\n    dead_code\n)]\n",
        "use external::Thing;\n\n",
        "/// Module documentation.\n#[path = \"other.rs\"]\nmod other;\n\n",
        "/// Alias documentation.\n#[allow(dead_code)]\ntype Value = u8;\n",
        "\n// Another alias section.\n\ntype Other = u16;\n",
    );
    let mut config = Config::default();
    config.grouping.join_related = true;
    assert_eq!(
        structural(source, &config).map(|(fixed, _)| return fixed),
        Ok(expected.to_owned())
    );
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn inline_modules_keep_major_item_boundaries() -> Result<(), Box<dyn Error>> {
    let source = "mod before;\npub(super) mod inline {}\nmod after;\n";
    assert_eq!(
        structural(source, &Config::default())?.0,
        "mod before;\n\npub(super) mod inline {}\n\nmod after;\n"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn out_of_line_modules_respect_disabled_compact_declarations() -> Result<(), Box<dyn Error>> {
    let source = "mod first;\npub(super) mod second;\n";
    let mut config = Config::default();
    config.items.compact_declarations = false;
    assert_eq!(
        structural(source, &config)?.0,
        "mod first;\n\npub(super) mod second;\n"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn final_controls_keep_after_block_boundaries() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    for item in first {\n        consume(item);\n    }\n    while ready() {\n        advance();\n    }\n    for item in last {\n        consume(item);\n    }\n}\n";
    let expected = "fn f() {\n    for item in first {\n        consume(item);\n    }\n\n    while ready() {\n        advance();\n    }\n\n    for item in last {\n        consume(item);\n    }\n}\n";
    assert_eq!(
        structural(source, &Config::only(&[Rule::AfterBlock]))?.0,
        expected
    );

    // Activating the final loop must not turn empty draining into a phase.
    let drains =
        "fn drain() {\n    for _ in first.drain(..) {}\n    for _ in second.drain(..) {}\n}\n";
    assert_eq!(structural(drains, &Config::default())?.0, drains);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn value_returning_controls_remain_tail_values() -> Result<(), Box<dyn Error>> {
    let source = "fn loop_value() -> u8 {\n    setup();\n    loop { break 42; }\n}\nfn if_value(flag: bool) -> u8 {\n    setup();\n    if flag { 1 } else { 2 }\n}\nfn match_value(flag: bool) -> u8 {\n    setup();\n    match flag { true => 1, false => 2 }\n}\n";
    let mut config = Config::only(&[Rule::Exit]);
    config.exits.tail = Tail::AlwaysSeparate;
    assert_eq!(
        structural(source, &config)?.0,
        source.replace("setup();\n    ", "setup();\n\n    ")
    );

    config.exits.tail = Tail::Preserve;
    assert_eq!(structural(source, &config)?.0, source);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn guards_accept_short_trailing_exits_and_try_operations() -> Result<(), Box<dyn Error>> {
    for (body, expected) in [
        ("return;", true),
        ("check()?;", true),
        ("check()?", true),
        ("let result = check(); result?;", true),
        ("log::warn!(\"failed\"); return None;", true),
        ("let _ = sender.send(()); return Ok(());", true),
        ("check()?; work();", false),
        ("prepare(); log(); return;", false),
        ("let later = || check()?;", false),
        ("let value = check()?;", false),
        ("work();", false),
        ("", false),
    ] {
        let source = format!("fn f() {{ if condition {{ {body} }} }}");
        let parsed = parse_source(&source, "2024")?;
        let unit = parsed
            .model
            .lists
            .iter()
            .flat_map(|list| return &list.units)
            .find(|unit| return unit.kind == UnitKind::Control)
            .ok_or("missing if unit")?;
        assert_eq!(unit.is_guard, expected, "{body}");
    }

    let source = "fn f() { if condition { check()?; } else { return; } }";
    let parsed = parse_source(source, "2024")?;
    assert!(
        !parsed
            .model
            .lists
            .iter()
            .flat_map(|list| return &list.units)
            .any(|unit| return unit.is_guard)
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn unsafe_operations_and_guard_checks_can_precede_bindings() -> Result<(), Box<dyn Error>> {
    for source in [
        "fn f() {\n    unsafe { configure(); }\n    let option = 0;\n    unsafe { set_option(option); }\n    let output = 0;\n}\n",
        "fn f() {\n    if failed { check()?; }\n    let output = 0;\n}\n",
        "fn f() {\n    if failed { log::warn!(\"failed\"); return; }\n    let output = 0;\n}\n",
    ] {
        let config = Config::only(&[Rule::AfterBlock]);
        assert_eq!(structural(source, &config)?.0, source);
        let mut strict_config = config.clone();
        strict_config.grouping.expressions = Expressions::Strict;
        assert!(!structural(source, &strict_config)?.1.findings.is_empty());
        let mut separate = config;
        separate.control_flow.related_continuation = false;
        separate.control_flow.guard_chain = GuardChain::Separate;
        assert!(!structural(source, &separate)?.1.findings.is_empty());
    }

    let plain = "fn f() {\n    { configure(); }\n    let option = 0;\n}\n";
    assert!(
        !structural(plain, &Config::only(&[Rule::AfterBlock]))?
            .1
            .findings
            .is_empty()
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn macro_following_block_with_shared_input_remains_compact() -> Result<(), Box<dyn Error>> {
    let source = "fn f(values: &[u32]) {\n    for value in values {\n        consume(value);\n    }\n    process!(values);\n}\n";
    let mut parsed = parse_source(source, "2024")?;
    let mut semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
    let loop_values = source
        .find("in values")
        .map(|p| return p + 3)
        .ok_or("token")?;
    let macro_values = source
        .find("process!(values)")
        .map(|p| return p + 9)
        .ok_or("token")?;

    let values_place = Place::local("values-id");
    semantics.events.push(Event {
        range: ByteRange::new(loop_values, loop_values + 6),
        kind: EventKind::Read,
        place: Some(values_place.clone()),
    });
    semantics.events.push(Event {
        range: ByteRange::new(macro_values, macro_values + 6),
        kind: EventKind::Read,
        place: Some(values_place.clone()),
    });
    parsed.attach(source, &semantics);
    let plan_shared = plan(&Config::default(), source, &parsed.model)?;
    assert!(plan_shared.findings.is_empty());

    let unrelated = "fn f(values: &[u32], other: &[u32]) {\n    for value in values {\n        consume(value);\n    }\n    process!(other);\n}\n";
    let mut parsed_unrelated = parse_source(unrelated, "2024")?;
    let mut semantics_unrelated = SemanticIndex::structural_anchors(parsed_unrelated.code_ranges());
    let loop_val = unrelated
        .find("in values")
        .map(|p| return p + 3)
        .ok_or("token")?;
    let macro_oth = unrelated
        .find("process!(other)")
        .map(|p| return p + 9)
        .ok_or("token")?;

    semantics_unrelated.events.push(Event {
        range: ByteRange::new(loop_val, loop_val + 6),
        kind: EventKind::Read,
        place: Some(values_place),
    });
    semantics_unrelated.events.push(Event {
        range: ByteRange::new(macro_oth, macro_oth + 5),
        kind: EventKind::Read,
        place: Some(Place::local("other-id")),
    });
    parsed_unrelated.attach(unrelated, &semantics_unrelated);
    let plan_unrelated = plan(&Config::default(), unrelated, &parsed_unrelated.model)?;
    assert_eq!(plan_unrelated.findings.len(), 1);
    assert_eq!(plan_unrelated.findings[0].rule, Rule::AfterBlock);

    return Ok(());
}
