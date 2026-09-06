use std::error::Error;

use super::*;

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
