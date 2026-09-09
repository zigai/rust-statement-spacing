use super::*;
use rust_statement_spacing_core::config::{Bindings, GuardChain, SelfFields, Tail};
use std::error::Error;
use std::fmt::Write as _;

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
    assert_eq!(
        structural(returning, &visual_config())?.0,
        returning.replace("    return Ok", "\n    return Ok")
    );

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
    assert_eq!(apply_edits(source, &result.edits())?, source);

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
fn matching_terminal_error_adapters_do_not_connect_unrelated_bindings() -> Result<(), Box<dyn Error>>
{
    let source = "fn f() {\n    let first = load(\n        left,\n    ).map_err(|error| Error::Read(error))?;\n    let second = load(\n        right,\n    ).map_err(Error::Read)?;\n    let third = load(\n        other,\n    ).map_err(Error::Write)?;\n}\n";
    assert_eq!(
        structural(source, &visual_config())?.0,
        source
            .replace("    let second", "\n    let second")
            .replace("    let third", "\n    let third")
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

fn normalized_pair(
    source: &str,
    config: &Config,
    first: &str,
    second: &str,
    facts: &[Facts; 2],
) -> Result<String, String> {
    let mut parsed = parse_source(source, "2024")?;
    let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
    parsed.attach(source, &anchors);

    for list in &mut parsed.model.lists {
        for unit in &mut list.units {
            let text = source.get(unit.code_range.as_range()).unwrap_or("");
            if text == first {
                unit.facts = facts[0].clone();
            } else if text == second {
                unit.facts = facts[1].clone();
            }
        }
    }

    let result = plan(config, source, &parsed.model)?;

    return apply_edits(source, &result.edits());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn selection_construction_and_conditional_configuration_remain_distinct()
-> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let selection = "let mut command = if available {\n        owned_command()\n    } else {\n        default_command()\n    };";
    let construction = "let mut process = spawn(\n        command.arg(input),\n    )?;";
    let configuration =
        "if let Some(output) = process.output.as_mut() {\n        output.log = Some(log);\n    }";

    let command = Place::local("command");
    let process = Place::local("process");
    let selection_facts = Facts {
        known: true,
        definitions: [command.clone()].into(),
        ..Facts::default()
    };

    let construction_facts = Facts {
        known: true,
        definitions: [process.clone()].into(),
        reads: [command.clone()].into(),
        mutating_receivers: [command].into(),
        ..Facts::default()
    };

    let configuration_facts = Facts {
        known: true,
        reads: [process.clone()].into(),
        header_reads: [process.clone()].into(),
        mutating_receivers: [process].into(),
        ..Facts::default()
    };

    for (first, second, facts, ending) in [
        (
            selection,
            construction,
            [selection_facts, construction_facts.clone()],
            "",
        ),
        (
            construction,
            configuration,
            [construction_facts, configuration_facts],
            "\n\n    return Ok(process);",
        ),
    ] {
        let expected = format!("fn f() {{\n    {first}\n\n    {second}{ending}\n}}\n");
        let compact = expected.replace("\n\n", "\n");

        for source in [&compact, &expected] {
            assert_eq!(
                normalized_pair(source, &config, first, second, &facts)?,
                expected
            );
        }
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn normalization_is_independent_of_optional_separators() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let producer = Facts {
        known: true,
        definitions: [Place::local("value")].into(),
        ..Facts::default()
    };

    let consumer = Facts {
        known: true,
        reads: [Place::local("value")].into(),
        ..Facts::default()
    };

    let parallel = Facts {
        known: true,
        reads: [Place::local("input")].into(),
        ..Facts::default()
    };

    let mutation = Facts {
        known: true,
        mutating_receivers: [Place::local("value")].into(),
        ..consumer.clone()
    };

    let validation = Facts {
        header_reads: [Place::local("value")].into(),
        ..consumer.clone()
    };
    let check = Facts {
        check_of: Some(Place::local("value")),
        ..validation.clone()
    };

    let destructuring = Facts {
        definitions: [Place::local("value"), Place::local("other")].into(),
        ..producer.clone()
    };

    for (first, second, facts) in [
        (
            "let left = 1;",
            "let right = 2;",
            [Facts::default(), Facts::default()],
        ),
        (
            "let value = load();",
            "let next = consume(value);",
            [producer.clone(), consumer.clone()],
        ),
        (
            "let (value, other) = split(\n        input,\n    );",
            "let next = consume(value);",
            [destructuring, consumer.clone()],
        ),
        (
            "let value = Record {\n        field: input,\n    };",
            "let next = consume(value);",
            [producer.clone(), consumer.clone()],
        ),
        (
            "let value = || {\n        compute()\n    };",
            "consume(value);",
            [producer.clone(), consumer.clone()],
        ),
        (
            "let Some(value) = load() else {\n        return;\n    };",
            "consume(value);",
            [producer.clone(), consumer.clone()],
        ),
        (
            "let left = load(\n        input,\n    ).map_err(Error::Read)?;",
            "let right = read(\n        input,\n    ).map_err(Error::Read)?;",
            [parallel.clone(), parallel],
        ),
        (
            "let value = probe(\n        input,\n    );",
            "if value.is_err() {\n        return;\n    }",
            [producer.clone(), check],
        ),
        (
            "let value = load();",
            "if invalid(value) {\n        return;\n    }",
            [producer.clone(), validation],
        ),
        (
            "let mut value = create();",
            "value.push(input);",
            [producer, mutation.clone()],
        ),
        ("value.push(input);", "value.flush();", [mutation, consumer]),
    ] {
        let compact = format!("fn f() {{\n    {first}\n    {second}\n}}\n");

        let spaced = format!("fn f() {{\n    {first}\n\n    {second}\n}}\n");

        for source in [&compact, &spaced] {
            let fixed = normalized_pair(source, &config, first, second, &facts)?;
            assert_eq!(fixed, compact, "{first}");
            assert_eq!(
                normalized_pair(&fixed, &config, first, second, &facts)?,
                fixed
            );
        }
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn normalization_respects_conditional_boundaries_and_comments() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    config.control_flow.related_continuation = true;
    let conditional = "if ready {\n        prepare();\n    } else {\n        recover();\n    }";

    for (next, separator) in [
        ("consume();", "\n"),
        ("Ok(value)", "\n"),
        ("return Ok(value);", "\n"),
    ] {
        let compact = format!("fn f() {{\n    {conditional}\n    {next}\n}}\n");

        let spaced = format!("fn f() {{\n    {conditional}\n\n    {next}\n}}\n");

        let expected = format!("fn f() {{\n    {conditional}\n{separator}    {next}\n}}\n");

        for source in [compact, spaced] {
            assert_eq!(structural(&source, &config)?.0, expected);
        }
    }

    let loop_body = "fn f() {\n    loop {\n        if ready {\n            prepare();\n        }\n        consume();\n    }\n}\n";
    assert_eq!(
        structural(loop_body, &config)?.0,
        loop_body.replace("        consume();", "\n        consume();")
    );

    let protected =
        "fn f() {\n    let left = 1;\n\n    // A separate section.\n    let right = 2;\n}\n";
    assert_eq!(structural(protected, &config)?.0, protected);

    config.grouping.bindings = Bindings::Preserve;
    let preserved = "fn f() {\n    let left = 1;\n\n    let right = 2;\n}\n";
    assert_eq!(structural(preserved, &config)?.0, preserved);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn normalization_does_not_join_unknown_or_suppressed_relationships() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let first = "let left = load(\n        left_input,\n    ).map_err(Error::Read)?;";
    let second = "let right = load(\n        right_input,\n    ).map_err(Error::Read)?;";
    let compact = format!("fn f() {{\n    {first}\n    {second}\n}}\n");

    let spaced = format!("fn f() {{\n    {first}\n\n    {second}\n}}\n");
    let unrelated = [
        Facts {
            known: true,
            reads: [Place::local("left")].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            reads: [Place::local("right")].into(),
            ..Facts::default()
        },
    ];

    for source in [&compact, &spaced] {
        assert_eq!(
            normalized_pair(source, &config, first, second, &unrelated)?,
            spaced
        );
    }

    let unknown = "fn f() {\n    opaque();\n\n    other();\n}\n";
    assert_eq!(structural(unknown, &config)?.0, unknown);

    let first = "let value = probe();";
    let second = "if value.is_err() {\n        return;\n    }";
    let checked = format!("fn f() {{\n    {first}\n\n    {second}\n}}\n");
    let facts = [
        Facts {
            known: true,
            definitions: [Place::local("value")].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            reads: [Place::local("value")].into(),
            header_reads: [Place::local("value")].into(),
            check_of: Some(Place::local("value")),
            ..Facts::default()
        },
    ];
    config.disable.push(Rule::ResultCheck);
    assert_eq!(
        normalized_pair(&checked, &config, first, second, &facts)?,
        checked
    );

    let conditional = "if ready {\n        value = compute();\n    }";
    let consumer = "consume(value);";
    let compact = format!("fn f() {{\n    {conditional}\n    {consumer}\n}}\n");
    let facts = [
        Facts {
            known: true,
            writes: [Place::local("value")].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            reads: [Place::local("value")].into(),
            ..Facts::default()
        },
    ];
    config.control_flow.related_continuation = true;
    assert_eq!(
        normalized_pair(&compact, &config, conditional, consumer, &facts)?,
        compact.replace("    consume(value);", "\n    consume(value);")
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn normalization_keeps_aggregate_validation_and_incidental_receivers_separate()
-> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;

    for (first, second, facts) in [
        (
            "let value = Record {\n        first: input,\n        second: other,\n    };",
            "validate(value)?;",
            [
                Facts {
                    known: true,
                    definitions: [Place::local("value")].into(),
                    ..Facts::default()
                },
                Facts {
                    known: true,
                    reads: [Place::local("value")].into(),
                    ..Facts::default()
                },
            ],
        ),
        (
            "write_secure(\n        staging.join(\"first\"),\n        input,\n    )?;",
            "materialize_profile(staging.join(\"second\"))?;",
            [
                Facts {
                    known: true,
                    receivers: [Place::local("staging")].into(),
                    reads: [Place::local("staging")].into(),
                    ..Facts::default()
                },
                Facts {
                    known: true,
                    receivers: [Place::local("staging")].into(),
                    reads: [Place::local("staging")].into(),
                    ..Facts::default()
                },
            ],
        ),
    ] {
        let compact = format!("fn f() {{\n    {first}\n    {second}\n}}\n");

        let spaced = format!("fn f() {{\n    {first}\n\n    {second}\n}}\n");

        for source in [&compact, &spaced] {
            assert_eq!(
                normalized_pair(source, &config, first, second, &facts)?,
                spaced
            );
        }
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn parallel_error_callbacks_can_share_captures_without_executing_them() -> Result<(), Box<dyn Error>>
{
    let mut config = visual_config();
    config.grouping.join_related = true;
    let first = "let width = cols.checked_add(\n        padding,\n    ).ok_or_else(|| Error::Size { cols, rows, attempts })?;";
    let second = "let height = rows.checked_add(\n        margin,\n    ).ok_or_else(|| Error::Size { cols, rows, attempts })?;";
    let compact = format!("fn f() {{\n    {first}\n    {second}\n}}\n");

    let spaced = format!("fn f() {{\n    {first}\n\n    {second}\n}}\n");
    let facts = [
        Facts {
            known: true,
            reads: [Place::local("cols")].into(),
            captures: [
                Place::local("cols"),
                Place::local("rows"),
                Place::local("attempts"),
            ]
            .into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            reads: [Place::local("rows")].into(),
            captures: [
                Place::local("cols"),
                Place::local("rows"),
                Place::local("attempts"),
            ]
            .into(),
            ..Facts::default()
        },
    ];

    for source in [&compact, &spaced] {
        assert_eq!(
            normalized_pair(source, &config, first, second, &facts)?,
            compact
        );
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn test_preparation_ends_before_assertions_not_before_extraction() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let producer = "let input = events.iter().find_map(|event| {\n        event.payload()\n    });";
    let extraction = "let (operation, bytes) = input.expect(\"input\");";
    let source = format!(
        "#[test]\nfn checks() {{\n    {producer}\n\n    {extraction}\n    assert_eq!(bytes, expected);\n\n    assert!(operation.is_some());\n    assert!(events.is_complete());\n}}\n"
    );

    let expected = format!(
        "#[test]\nfn checks() {{\n    {producer}\n    {extraction}\n\n    assert_eq!(bytes, expected);\n    assert!(operation.is_some());\n    assert!(events.is_complete());\n}}\n"
    );

    let facts = [
        Facts {
            definitions: [Place::local("input")].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            reads: [Place::local("input")].into(),
            ..Facts::default()
        },
    ];

    for input in [&source, &expected] {
        let fixed = normalized_pair(input, &config, producer, extraction, &facts)?;
        assert_eq!(fixed, expected);
        assert_eq!(
            token_fingerprint(input, "2024")?,
            token_fingerprint(&fixed, "2024")?
        );
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn test_action_checks_stay_attached_between_assertion_phases() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let action = "let response = driver.request(Detach).expect(\"response\");";
    let check = "assert!(matches!(response.result, Success));";
    let source = format!(
        "#[test]\nfn checks() {{\n    assert_eq!(pids[0], pids[1]);\n    assert_ne!(pids[0], 0);\n    {action}\n\n    {check}\n    driver.close().expect(\"close\");\n}}\n"
    );

    let expected = format!(
        "#[test]\nfn checks() {{\n    assert_eq!(pids[0], pids[1]);\n    assert_ne!(pids[0], 0);\n\n    {action}\n    {check}\n\n    driver.close().expect(\"close\");\n}}\n"
    );

    let facts = [
        Facts {
            known: true,
            definitions: [Place::local("response")].into(),
            ..Facts::default()
        },
        Facts {
            reads: [Place::local("response")].into(),
            ..Facts::default()
        },
    ];

    for input in [&source, &expected] {
        assert_eq!(
            normalized_pair(input, &config, action, check, &facts)?,
            expected
        );
    }

    let loop_source = format!(
        "#[test]\nfn checks() {{\n    prepare();\n    prepare();\n    prepare();\n    prepare();\n    for item in items {{\n        {action}\n\n        {check}\n    }}\n}}\n"
    );

    let fixed = normalized_pair(&loop_source, &config, action, check, &facts)?;
    assert!(fixed.contains(&format!("{action}\n        {check}")));

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn assertion_phases_respect_test_scope_threshold_and_macro_shape() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let tiny = "#[test]\nfn checks() {\n    let value = 1;\n    assert_eq!(value, 1);\n    assert!(true);\n    assert_ne!(value, 0);\n}\n";
    assert_eq!(structural(tiny, &config)?.0, tiny);

    config.exits.short_block_max_statements = 3;
    let spaced = tiny.replace("    assert_eq!", "\n    assert_eq!");
    assert_eq!(structural(tiny, &config)?.0, spaced);
    assert_eq!(structural(&spaced, &config)?.0, spaced);

    let ordinary = tiny.replace("#[test]\n", "");
    assert_eq!(structural(&ordinary, &config)?.0, ordinary);

    let methods = "#[test]\nfn checks() {\n    let value = 1;\n    checker.assert(value);\n    checker.assert_eq(value);\n    checker.assert_ne(value);\n}\n";
    let ordinary_methods = methods.replace("#[test]\n", "");
    assert_eq!(
        structural(methods, &config)?.0,
        format!("#[test]\n{}", structural(&ordinary_methods, &config)?.0)
    );

    let disabled = Config {
        disable: vec![Rule::Bindings, Rule::Expressions],
        ..config.clone()
    };
    assert_eq!(structural(tiny, &disabled)?.0, tiny);

    let protected = tiny.replace("#[test]", "#[test]\n#[rustfmt::skip]");
    assert_eq!(structural(&protected, &config)?.0, protected);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn standalone_assertion_operations_end_before_cleanup() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let source = "#[test]\nfn checks() {\n    assert!(ready);\n    assert!(attached);\n    assert!(alive);\n    assert!(Command::new(\"kill\").arg(pid).status().expect(\"kill\").success());\n    let _ = driver.close();\n}\n";
    let expected = source.replace("    let _", "\n    let _");
    assert_eq!(structural(source, &config)?.0, expected);
    assert_eq!(structural(&expected, &config)?.0, expected);

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn action_assertion_and_shared_input_verification_form_one_step() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let action = "let response = driver.request(destination).expect(\"response\");";
    let check = "assert!(matches!(response.result, Success));";
    let verification = "validate(&session, &destination).expect(\"valid\");";
    let source = format!(
        "#[test]\nfn checks() {{\n    assert!(ready);\n    assert!(attached);\n\n    {action}\n    {check}\n    {verification}\n}}\n"
    );

    for input in [
        &source,
        &source.replace(verification, &format!("\n    {verification}")),
    ] {
        let mut parsed = parse_source(input, "2024")?;
        let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
        parsed.attach(input, &anchors);

        for list in &mut parsed.model.lists {
            for unit in &mut list.units {
                let text = input.get(unit.code_range.as_range()).unwrap_or("");
                if text == action {
                    unit.facts.definitions.insert(Place::local("response"));
                    unit.facts.reads.insert(Place::local("destination"));
                } else if text == check {
                    unit.facts.reads.insert(Place::local("response"));
                } else if text == verification {
                    unit.facts.reads.insert(Place::local("destination"));
                }
            }
        }

        let result = plan(&config, input, &parsed.model)?;
        assert_eq!(apply_edits(input, &result.edits())?, source);
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_immediate_guards_and_preparation_pairs() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;

    for (first, second, definitions, reads) in [
        (
            "let status = response.status();",
            "if !status.is_success() {\n        return;\n    }",
            "status",
            "status",
        ),
        (
            "let names = extract_define_names(&content);",
            "if names.is_empty() {\n        continue;\n    }",
            "names",
            "names",
        ),
        (
            "let product = candidate * width;",
            "if product < threshold {\n        continue;\n    }",
            "product",
            "product",
        ),
        (
            "let t = input.trim();",
            "let hex = t.strip_prefix('#').unwrap_or(t);",
            "t",
            "t",
        ),
    ] {
        let facts = [
            Facts {
                known: true,
                definitions: [Place::local(definitions)].into(),
                ..Facts::default()
            },
            Facts {
                known: true,
                reads: [Place::local(reads)].into(),
                header_reads: [Place::local(reads)].into(),
                ..Facts::default()
            },
        ];

        let expected = format!("fn f() {{\n    {first}\n    {second}\n}}\n");

        for gap in ["\n", "\n\n"] {
            let source = format!("fn f() {{\n    {first}{gap}    {second}\n}}\n");
            assert_eq!(
                normalized_pair(&source, &config, first, second, &facts)?,
                expected
            );
        }
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_constants_keep_multiline_declarations_separate() -> Result<(), Box<dyn Error>> {
    let mut config = Config::default();
    config.grouping.join_related = true;
    let source = "const A: &str = r#\"first\nsecond\"#;\nconst B: &str = r#\"third\nfourth\"#;\n";
    let expected = source.replace("\nconst B", "\n\nconst B");

    for input in [source, expected.as_str()] {
        assert_eq!(structural(input, &config)?.0, expected);
    }

    assert_eq!(
        structural("const A: u32 = 1;\n\nconst B: u32 = 2;\n", &config)?.0,
        "const A: u32 = 1;\nconst B: u32 = 2;\n"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_conditional_repeat_requires_same_action_and_receiver() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let action = "sim.add_card(side);";

    for (body, receiver, joined) in [
        (action, "sim", true),
        ("sim.remove_card(side);", "sim", false),
        ("other.add_card(side);", "other", false),
    ] {
        let conditional = format!("if sim.golden {{\n        {body}\n    }}");
        let source = format!("fn f() {{\n    {action}\n\n    {conditional}\n}}\n");
        let facts = [
            Facts {
                known: true,
                receivers: [Place::local("sim")].into(),
                reads: [Place::local("sim")].into(),
                ..Facts::default()
            },
            Facts {
                known: true,
                receivers: [Place::local(receiver)].into(),
                ..Facts::default()
            },
        ];

        let expected = if joined {
            source.replace("\n\n", "\n")
        } else {
            source.clone()
        };

        for input in [&source, &expected] {
            assert_eq!(
                normalized_pair(input, &config, action, &conditional, &facts)?,
                expected
            );
        }
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_iterative_action_assertion_rounds() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let action = "fs::write(bundle.join(\"extra\"), b\"extra\").expect(\"write\");";
    let check = "assert!(verify(&bundle).is_err());";
    let expected = format!(
        "#[test]\nfn f() {{\n    {action}\n    {check}\n\n    {action}\n    {check}\n\n    {action}\n    {check}\n}}\n"
    );

    let facts = [
        Facts {
            known: true,
            reads: [Place::local("bundle")].into(),
            ..Facts::default()
        },
        Facts {
            reads: [Place::local("bundle")].into(),
            ..Facts::default()
        },
    ];

    let padded = expected.replace(&format!("\n    {check}"), &format!("\n\n    {check}"));
    for input in [&expected, &padded] {
        assert_eq!(
            normalized_pair(input, &config, action, check, &facts)?,
            expected
        );
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_distinct_fallible_validators_and_ui_assignments() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let first = "validate_target(target)?;";
    let second = "validate_display_name(name)?;";
    let facts = [
        Facts {
            known: true,
            direct_callees: ["target-validator".into()].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            direct_callees: ["name-validator".into()].into(),
            ..Facts::default()
        },
    ];
    let expected = format!("fn f() {{\n    {first}\n    {second}\n}}\n");

    assert_eq!(
        normalized_pair(
            &expected.replace("    validate_display", "\n    validate_display"),
            &config,
            first,
            second,
            &facts
        )?,
        expected
    );

    let first =
        "dirty |= ui\n        .checkbox(&mut cfg.stop_on_win, \"Stop\")\n        .changed();";
    let second =
        "dirty |= ui\n        .checkbox(&mut cfg.despawn, \"Despawn\")\n        .changed();";
    let facts = [
        Facts {
            known: true,
            writes: [Place::local("dirty")].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            writes: [Place::local("dirty")].into(),
            ..Facts::default()
        },
    ];
    let expected = format!("fn f() {{\n    {first}\n    {second}\n}}\n");

    assert_eq!(
        normalized_pair(
            &expected.replace(second, &format!("\n    {second}")),
            &config,
            first,
            second,
            &facts
        )?,
        expected
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_accumulator_suffix_is_bounded_and_separate_from_geometry() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;

    for (setup, control, names) in [
        (
            "let mut masks = Vec::new();",
            "for clip in clips {\n        for frame in clip {\n            masks.push(frame);\n        }\n    }",
            vec!["masks"],
        ),
        (
            "let mut result = String::new();\n    let mut last_dash = false;",
            "for character in value.chars() {\n        result.push(character);\n        last_dash = character == '-';\n    }",
            vec!["result", "last_dash"],
        ),
        (
            "let mut attack = 0_i32;\n    let mut health = 0_i32;",
            "if target.is_beast() {\n        attack += 1;\n        health += 1;\n    }",
            vec!["attack", "health"],
        ),
    ] {
        let expected = format!(
            "fn f() {{\n    let x_origin = width / 2;\n    let y_origin = height / 2;\n\n    {setup}\n    {control}\n}}\n"
        );

        for input in [
            &expected,
            &expected
                .replace("\n\n", "\n")
                .replace(&format!("    {control}"), &format!("\n    {control}")),
        ] {
            let mut parsed = parse_source(input, "2024")?;
            let mut semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
            for name in &names {
                let declaration = format!("let mut {name}");
                let start =
                    input.find(&declaration).ok_or("missing declaration")? + "let mut ".len();
                semantics.events.push(Event {
                    range: ByteRange::new(start, start + name.len()),
                    kind: EventKind::Define,
                    place: Some(Place::local(*name)),
                });
                let start = input.rfind(name).ok_or("missing mutation")?;
                semantics.events.push(Event {
                    range: ByteRange::new(start, start + name.len()),
                    kind: EventKind::Write,
                    place: Some(Place::local(*name)),
                });
            }

            parsed.attach(input, &semantics);
            let result = plan(&config, input, &parsed.model)?;
            assert_eq!(apply_edits(input, &result.edits())?, expected);
        }
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_accumulator_exception_respects_scope_and_rule_settings() -> Result<(), Box<dyn Error>> {
    use rust_statement_spacing_core::config::UseIn;

    let setup = "let mut masks = Vec::new();";
    let control = "for clip in clips {\n        masks.push(clip);\n    }";
    let source = format!("fn f() {{\n    {setup}\n\n    {control}\n}}\n");
    let facts = [
        Facts {
            known: true,
            definitions: [Place::local("masks")].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            mutating_receivers: [Place::local("masks")].into(),
            ..Facts::default()
        },
    ];

    let mut config = visual_config();
    config.grouping.join_related = true;
    let mut zero = config.clone();
    zero.grouping.max_before_control = 0;
    let mut header = config.clone();
    header.grouping.use_in = UseIn::Header;
    let mut preserve = config.clone();
    preserve.grouping.bindings = Bindings::Preserve;
    let mut disabled = config.clone();
    disabled.disable.push(Rule::ControlFlow);
    for policy in [zero, header, preserve, disabled] {
        assert_eq!(
            normalized_pair(&source, &policy, setup, control, &facts)?,
            source
        );
    }

    assert_eq!(
        normalized_pair(&source, &config, setup, control, &facts)?,
        source.replace("\n\n", "\n")
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_large_accumulator_setup_retains_overflow_boundary() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;

    for count in [3, 4, 5] {
        let mut source = String::from("fn f() {\n");
        for index in 0..count {
            writeln!(source, "    let mut count_{index} = 0;")?;
        }

        source.push_str("    for value in values {\n");
        for index in 0..count {
            writeln!(source, "        count_{index} += value;")?;
        }

        source.push_str("    }\n}\n");

        let mut parsed = parse_source(&source, "2024")?;
        let mut semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
        for index in 0..count {
            let name = format!("count_{index}");

            for (start, kind) in [
                (
                    source.find(&name).ok_or("missing definition")?,
                    EventKind::Define,
                ),
                (
                    source.rfind(&name).ok_or("missing mutation")?,
                    EventKind::Write,
                ),
            ] {
                semantics.events.push(Event {
                    range: ByteRange::new(start, start + name.len()),
                    kind,
                    place: Some(Place::local(&name)),
                });
            }
        }

        parsed.attach(&source, &semantics);

        let expected = if count > 4 {
            source.replace("    for value", "\n    for value")
        } else {
            source.clone()
        };
        assert_eq!(
            apply_edits(&source, &plan(&config, &source, &parsed.model)?.edits())?,
            expected
        );
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_following_control_does_not_split_preparation() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let first = "let t = input.trim();";
    let second = "let hex = t.strip_prefix('#').unwrap_or(t);";

    for (control, separation) in [
        ("if hex.len() == 6 {\n        consume(hex);\n    }", "\n"),
        ("if hex.is_empty() {\n        return;\n    }", ""),
    ] {
        let expected =
            format!("fn f() {{\n    {first}\n    {second}\n{separation}    {control}\n}}\n");

        for input in [
            &expected,
            &expected.replace(&format!("    {second}"), &format!("\n    {second}")),
        ] {
            let mut parsed = parse_source(input, "2024")?;
            let anchors = SemanticIndex::structural_anchors(parsed.code_ranges());
            parsed.attach(input, &anchors);

            for list in &mut parsed.model.lists {
                for unit in &mut list.units {
                    let text = input.get(unit.code_range.as_range()).unwrap_or("");
                    if text == first {
                        unit.facts.definitions.insert(Place::local("t"));
                    } else if text == second {
                        unit.facts.definitions.insert(Place::local("hex"));
                        unit.facts.reads.insert(Place::local("t"));
                    } else if text == control {
                        unit.facts.header_reads.insert(Place::local("hex"));
                    }
                }
            }

            assert_eq!(
                apply_edits(input, &plan(&config, input, &parsed.model)?.edits())?,
                expected
            );
        }
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn audit_conditional_repeat_excludes_pattern_shadowing() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let action = "sim.add_card(side);";
    let conditional = "if let Some(sim) = sim.next() {\n        sim.add_card(side);\n    }";
    let source = format!("fn f() {{\n    {action}\n\n    {conditional}\n}}\n");
    let facts = [
        Facts {
            known: true,
            receivers: [Place::local("outer:sim")].into(),
            ..Facts::default()
        },
        Facts {
            known: true,
            receivers: [Place::local("outer:sim"), Place::local("inner:sim")].into(),
            ..Facts::default()
        },
    ];
    assert_eq!(
        normalized_pair(&source, &config, action, conditional, &facts)?,
        source
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn mutation_assertions_require_the_changed_value() -> Result<(), Box<dyn Error>> {
    let mut config = visual_config();
    config.grouping.join_related = true;
    let check = "assert!(value.is_empty());";

    for action in ["value = empty();", "value.clear();"] {
        for related in [true, false] {
            let target = Place::local(if related { "value" } else { "other" });

            let mut action_facts = Facts::default();
            if action.contains("clear") {
                action_facts.mutating_receivers.insert(target);
            } else {
                action_facts.writes.insert(target);
            }

            let facts = [
                action_facts,
                Facts {
                    reads: [Place::local("value")].into(),
                    ..Facts::default()
                },
            ];

            let gap = if related { "" } else { "\n" };
            let expected = format!(
                "#[test]\nfn f() {{\n    {action}\n{gap}    {check}\n\n    {action}\n{gap}    {check}\n\n    {action}\n{gap}    {check}\n}}\n"
            );

            for input in [&expected, &expected.replace("\n\n", "\n")] {
                assert_eq!(
                    normalized_pair(input, &config, action, check, &facts)?,
                    expected
                );
            }
        }
    }

    return Ok(());
}
