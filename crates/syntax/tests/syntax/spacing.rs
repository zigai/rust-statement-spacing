use std::error::Error;

use rust_statement_spacing_core::config::{Bindings, GuardChain, SelfFields, Tail};

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

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn multiline_phases_preserve_short_bindings_and_comment_attachments() -> Result<(), Box<dyn Error>>
{
    let source = "fn f() {\n    let seed = 1;\n    let first = build(\n        seed,\n    );\n    // Prepare another value.\n    let second = build(\n        2,\n    );\n    publish(\n        first, second,\n    );\n    finish();\n    cleanup();\n}\n";
    let expected = "fn f() {\n    let seed = 1;\n    let first = build(\n        seed,\n    );\n\n    // Prepare another value.\n    let second = build(\n        2,\n    );\n\n    publish(\n        first, second,\n    );\n\n    finish();\n\n    cleanup();\n}\n";
    let mut config = Config::default();
    config.grouping.bindings = Bindings::Multiline;
    config.grouping.expressions = Expressions::Multiline;
    assert_eq!(structural(source, &config)?.0, expected);
    assert!(structural(expected, &config)?.1.findings.is_empty());
    config.grouping.bindings = Bindings::Preserve;
    config.grouping.expressions = Expressions::Preserve;
    assert_eq!(structural(source, &config)?.0, source);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn multiline_short_consumers_require_resolved_dependency() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    let value =\n        build(|| compute());\n    let next = consume(value);\n}\n";
    let mut config = Config::default();
    config.grouping.bindings = Bindings::Multiline;
    for consumer in ["value", "outer:value"] {
        let mut parsed = parse_source(source, "2024")?;
        let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
        parsed.attach(source, &anchors);
        for list in &mut parsed.model.lists {
            for unit in &mut list.units {
                if source
                    .get(unit.code_range.as_range())
                    .is_some_and(|text| return text.starts_with("let value"))
                {
                    unit.facts.definitions.insert(Place::local("value"));
                } else if unit.kind == UnitKind::Let {
                    unit.facts.reads.insert(Place::local(consumer));
                }
            }
        }
        let result = plan(&config, source, &parsed.model)?;
        let fixed = apply_edits(source, &result.edits())?;
        let expected = if consumer == "value" {
            source.to_owned()
        } else {
            source.replace("    let next", "\n    let next")
        };
        assert_eq!(fixed, expected);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn multiline_control_spacing_respects_checks_and_rule_suppression() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    let result = probe(\n        input,\n    );\n    if let Err(error) = result {\n        return;\n    }\n}\n";
    for (immediate_check, disabled) in [(false, false), (true, false), (false, true)] {
        let mut config = visual_config();
        if disabled {
            config.disable.push(Rule::ControlFlow);
        }
        let mut parsed = parse_source(source, "2024")?;
        let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
        parsed.attach(source, &anchors);
        for list in &mut parsed.model.lists {
            for unit in &mut list.units {
                unit.facts.known = true;
                if unit.kind == UnitKind::Let {
                    unit.facts.definitions.insert(Place::local("result"));
                } else if unit.kind == UnitKind::Control {
                    unit.facts.reads.insert(Place::local("result"));
                    unit.facts.header_reads.insert(Place::local("result"));
                    unit.facts.check_of = immediate_check.then(|| return Place::local("result"));
                }
            }
        }
        let result = plan(&config, source, &parsed.model)?;
        let expected = if immediate_check || disabled {
            source.to_owned()
        } else {
            source.replace("    if let", "\n    if let")
        };
        assert_eq!(apply_edits(source, &result.edits())?, expected);
    }
    return Ok(());
}

fn visual_config() -> Config {
    let mut config = Config::default();
    config.grouping.bindings = Bindings::Multiline;
    config.grouping.expressions = Expressions::Multiline;
    config.grouping.self_fields = SelfFields::Distinct;
    config.control_flow.guard_chain = GuardChain::Contextual;
    config.control_flow.related_continuation = false;
    config.exits.tail = Tail::Visual;
    return config;
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn contextual_guards_distinguish_exiting_from_fallible_continuations() -> Result<(), Box<dyn Error>>
{
    let source = "fn f() {\n    if invalid {\n        return;\n    }\n    prepare();\n}\n";
    assert_eq!(
        structural(source, &visual_config())?.0,
        source.replace("    prepare();", "\n    prepare();")
    );

    let fallible = "fn f() {\n    if needed {\n        validate()?;\n    }\n    finish();\n}\n";
    assert_eq!(
        structural(fallible, &visual_config())?.0,
        fallible.replace("    finish();", "\n    finish();")
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn contextual_if_boundary_allows_only_explicit_returns() -> Result<(), Box<dyn Error>> {
    let prefix = "fn f() {\n    if invalid {\n        return Err(error);\n    }\n";
    for (next, separate) in [
        ("let path = root.join(\"key\");", true),
        ("prepare();", true),
        ("if missing {\n        return Err(error);\n    }", true),
        ("Ok(value)", true),
        ("return Ok(value);", false),
        ("return;", false),
    ] {
        let source = format!("{prefix}    {next}\n}}\n");
        let expected = format!(
            "{prefix}{}    {next}\n}}\n",
            if separate { "\n" } else { "" }
        );
        assert_eq!(structural(&source, &visual_config())?.0, expected);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn contextual_if_spacing_applies_in_loops_and_after_else() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    for value in values {\n        if skip {\n            continue;\n        }\n        consume(value);\n    }\n}\n";
    assert_eq!(
        structural(source, &visual_config())?.0,
        source.replace("        consume(value);", "\n        consume(value);")
    );
    let returning = "fn f() {\n    if ready {\n        prepare();\n    } else {\n        recover();\n    }\n    return Ok(());\n}\n";
    assert_eq!(structural(returning, &visual_config())?.0, returning);
    let continuing = returning.replace("return Ok(());", "finish();");
    assert_eq!(
        structural(&continuing, &visual_config())?.0,
        continuing.replace("    finish();", "\n    finish();")
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn contextual_validation_stages_and_call_checks_respect_shared_inputs() -> Result<(), Box<dyn Error>>
{
    let stages = "fn f() {\n    if missing(input) {\n        return;\n    }\n    prepare(input);\n    if invalid(input) {\n        return;\n    }\n    finish(input);\n}\n";
    let short = "fn f() {\n    prepare(input);\n    if invalid(input) {\n        return;\n    }\n    finish(input);\n}\n";
    let direct = "fn f() {\n    prepare(input);\n    if let Err(error) = validate(input) {\n        return;\n    }\n    finish(input);\n}\n";
    for (source, expected) in [
        (
            stages,
            stages
                .replace("    if invalid", "\n    if invalid")
                .replace("    prepare(input);", "\n    prepare(input);")
                .replace("    finish(input);", "\n    finish(input);"),
        ),
        (
            short,
            short.replace("    finish(input);", "\n    finish(input);"),
        ),
        (
            direct,
            direct
                .replace("    if let", "\n    if let")
                .replace("    finish(input);", "\n    finish(input);"),
        ),
    ] {
        let mut parsed = parse_source(source, "2024")?;
        let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
        parsed.attach(source, &anchors);
        for list in &mut parsed.model.lists {
            for unit in &mut list.units {
                unit.facts.known = true;
                unit.facts.reads.insert(Place::local("input"));
                if unit.kind == UnitKind::Control {
                    unit.facts.header_reads.insert(Place::local("input"));
                }
            }
        }
        let result = plan(&visual_config(), source, &parsed.model)?;
        assert_eq!(apply_edits(source, &result.edits())?, expected);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn literal_loop_setup_stays_whole_when_exceeding_allowance() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    let mut total = 0;\n    let mut count = 0;\n    for value in values {\n        total += value;\n        count += 1;\n    }\n    (total, count)\n}\n";
    let mut parsed = parse_source(source, "2024")?;
    let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
    parsed.attach(source, &anchors);
    for list in &mut parsed.model.lists {
        for unit in &mut list.units {
            let text = source.get(unit.code_range.as_range()).unwrap_or("");
            if text.starts_with("let mut total") {
                unit.facts.definitions.insert(Place::local("total"));
            } else if text.starts_with("let mut count") {
                unit.facts.definitions.insert(Place::local("count"));
            } else if unit.kind == UnitKind::Control {
                unit.facts
                    .writes
                    .extend([Place::local("total"), Place::local("count")]);
                unit.facts
                    .whole_body_reads
                    .extend([Place::local("total"), Place::local("count")]);
            }
        }
    }
    let mut config = visual_config();
    config.grouping.max_before_control = 2;
    let result = plan(&config, source, &parsed.model)?;
    assert_eq!(apply_edits(source, &result.edits())?, source);
    config.grouping.max_before_control = 1;
    let result = plan(&config, source, &parsed.model)?;
    assert_eq!(
        apply_edits(source, &result.edits())?,
        source.replace("    for value", "\n    for value")
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn visual_tails_distinguish_compact_values_from_completion_calls() -> Result<(), Box<dyn Error>> {
    let tuple = "fn f() {\n    prepare();\n    (left, right)\n}\n";
    assert_eq!(structural(tuple, &visual_config())?.0, tuple);
    let closure = "fn f() {\n    let run = || {\n        if invalid {\n            return Err(error);\n        }\n        Ok(value)\n    };\n}\n";
    assert_eq!(
        structural(closure, &visual_config())?.0,
        closure.replace("        Ok(value)", "\n        Ok(value)")
    );
    let guard = "fn f() {\n    prepare();\n    if invalid {\n        return;\n    }\n}\n";
    let mut exit_only = Config::only(&[Rule::Exit]);
    exit_only.exits.tail = Tail::Visual;
    assert_eq!(structural(guard, &exit_only)?.0, guard);
    let completion = "fn f() {\n    prepare();\n    Ok(value)\n}\n";
    assert_eq!(
        structural(completion, &visual_config())?.0,
        completion.replace("    Ok(value)", "\n    Ok(value)")
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn multiline_bindings_pair_only_matching_terminal_error_adapters() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    let first = load(\n        left,\n    ).map_err(|error| Error::Read(error))?;\n    let second = load(\n        right,\n    ).map_err(Error::Read)?;\n    let third = load(\n        other,\n    ).map_err(Error::Write)?;\n}\n";
    assert_eq!(
        structural(source, &visual_config())?.0,
        source.replace("    let third", "\n    let third")
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn final_macro_after_small_loop_is_compact_and_opaque() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    for value in values {\n        consume(value);\n    }\n    finish!({\n        first();\n        second();\n\n\n        third();\n    })\n}\n";
    assert_eq!(structural(source, &visual_config())?.0, source);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn multiline_destructuring_consumer_uses_positive_facts_when_effects_are_unknown()
-> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    let (left, right) = split(\n        || deferred(),\n    );\n    let result = combine(left, right);\n}\n";
    let mut parsed = parse_source(source, "2024")?;
    let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
    parsed.attach(source, &anchors);
    for list in &mut parsed.model.lists {
        for unit in &mut list.units {
            unit.facts.known = false;
            let text = source.get(unit.code_range.as_range()).unwrap_or("");
            if text.starts_with("let (left, right)") {
                unit.facts.definitions.insert(Place::local("left"));
                unit.facts.definitions.insert(Place::local("right"));
            } else if text.starts_with("let result") {
                unit.facts.reads.insert(Place::local("left"));
                unit.facts.reads.insert(Place::local("right"));
            }
        }
    }
    let config = visual_config();
    let result = plan(&config, source, &parsed.model)?;
    assert_eq!(apply_edits(source, &result.edits())?, source);
    for list in &mut parsed.model.lists {
        for unit in &mut list.units {
            unit.facts.reads.clear();
        }
    }
    let unrelated = plan(&config, source, &parsed.model)?;
    assert_eq!(
        apply_edits(source, &unrelated.edits())?,
        source.replace("    let result", "\n    let result")
    );
    return Ok(());
}
