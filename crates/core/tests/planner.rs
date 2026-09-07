//! Behavioral regression tests for spacing policy.

use std::collections::BTreeSet;
use std::error::Error;

use rust_statement_spacing_core::config::{
    Expressions, GuardChain, Overflow, SelfFields, Separation, Tail,
};
use rust_statement_spacing_core::*;
use std::fmt::Write as _;

fn places(names: &[&str]) -> BTreeSet<Place> {
    return names
        .iter()
        .map(|name| return Place::local(*name))
        .collect();
}

fn facts(defs: &[&str], reads: &[&str]) -> Facts {
    return Facts {
        known: true,
        definitions: places(defs),
        reads: places(reads),
        ..Facts::default()
    };
}

/// Synthetic units exercise the policy independently of rustc and the parser.
fn model(kinds: &[UnitKind], semantics: &[Facts], blanks: &[usize]) -> (String, SourceModel) {
    assert_eq!(kinds.len(), semantics.len());
    assert_eq!(blanks.len(), kinds.len().saturating_sub(1));
    let mut source = String::new();
    let mut units = Vec::new();
    let mut gaps = Vec::new();
    for (index, (&kind, facts)) in kinds.iter().zip(semantics).enumerate() {
        if index != 0 {
            let start = source.len();
            let blank_count = blanks.get(index - 1).copied().unwrap_or(0);
            source.push_str(&"\n".repeat(blank_count + 1));
            source.push_str("    ");
            gaps.push(Gap {
                range: ByteRange::new(start, source.len()),
                blank_lines: blank_count,
                vertical: true,
                protected: false,
                joinable: true,
            });
        }
        let start = source.len();
        let _ = write!(source, "unit{index};");
        let range = ByteRange::new(start, source.len());
        units.push(Unit {
            range,
            code_range: range,
            kind,
            facts: facts.clone(),
            is_tail: false,
            is_guard: false,
            is_empty_loop: false,
            is_loop_exit: false,
            is_bare_return: false,
            active: true,
            protected: false,
            enabled: RuleMask::all(),
            anchor: index,
        });
    }
    let list = UnitList {
        units,
        gaps,
        executable_count: kinds.len(),
        item_list: false,
    };
    return (
        source,
        SourceModel {
            lists: vec![list],
            layout_edges: vec![],
        },
    );
}

fn count(config: &Config, source: &str, model: &SourceModel) -> Result<usize, String> {
    return Ok(plan(config, source, model)?.findings.len());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn consecutive_bindings_are_compact() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Let],
        &[facts(&["a"], &[]), facts(&["b"], &[])],
        &[0],
    );
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn unrelated_binding_to_expression_is_separated() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&[], &[])],
        &[0],
    );
    let plan = plan(&Config::default(), &source, &model)?;
    assert_eq!(plan.findings.len(), 1);
    assert_eq!(plan.findings[0].rule, Rule::Bindings);
    assert_eq!(apply_edits(&source, &plan.edits())?, "unit0;\n\n    unit1;");
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn immediate_producer_consumer_is_compact() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&[], &["a"])],
        &[0],
    );
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn existing_optional_blank_is_preserved() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&[], &["a"])],
        &[1],
    );
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn explicitly_enabled_join_removes_optional_blank() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&[], &["a"])],
        &[1],
    );
    let mut config = Config::default();
    config.grouping.join_related = true;
    let plan = plan(&config, &source, &model)?;
    assert_eq!(apply_edits(&source, &plan.edits())?, "unit0;\n    unit1;");
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn unknown_facts_do_not_mean_unrelated() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), Facts::default()],
        &[0],
    );
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn self_field_input_chain_is_compact_unless_strictly_configured() -> Result<(), Box<dyn Error>> {
    let inputs = ["vm_dir", "arch", "machine"].map(|field| {
        return Facts {
            known: true,
            reads: BTreeSet::from([Place {
                local: "self-id".into(),
                projections: vec![field.into()],
                is_self: true,
            }]),
            ..Facts::default()
        };
    });
    let (source, model) = model(&[UnitKind::Expression; 3], &inputs, &[0, 0]);
    let config = Config::default();
    assert_eq!(count(&config, &source, &model)?, 0);
    let mut distinct = config.clone();
    distinct.grouping.self_fields = SelfFields::Distinct;
    let mut strict = config.clone();
    strict.grouping.expressions = Expressions::Strict;
    let mut no_shared_inputs = config;
    no_shared_inputs.grouping.shared_inputs = false;
    for (setting, config) in [
        ("distinct self fields", distinct),
        ("strict expressions", strict),
        ("no shared inputs", no_shared_inputs),
    ] {
        let result = plan(&config, &source, &model)?;
        assert_eq!(result.findings.len(), 2, "{setting}");
        assert_eq!(
            apply_edits(&source, &result.edits())?,
            "unit0;\n\n    unit1;\n\n    unit2;",
            "{setting}"
        );
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn disabled_specific_rule_is_not_reintroduced_by_generic_rule() -> Result<(), Box<dyn Error>> {
    let (source, mut model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&[], &[])],
        &[0],
    );
    model.lists[0].units[1].enabled = RuleMask::all().without(Rule::Bindings);
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn inactive_and_protected_units_are_barriers() -> Result<(), Box<dyn Error>> {
    let (source, mut model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&[], &[])],
        &[0],
    );
    model.lists[0].units[0].active = false;
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    model.lists[0].units[0].active = true;
    model.lists[0].gaps[0].protected = true;
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn control_attaches_one_related_setup() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["items"]);
    control.header_reads = places(&["items"]);
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Control],
        &[facts(&["items"], &[]), control],
        &[0],
    );
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn related_suffix_is_split_once() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["ready"]);
    control.header_reads = places(&["ready"]);
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Let, UnitKind::Control],
        &[facts(&["other"], &[]), facts(&["ready"], &[]), control],
        &[0, 0],
    );
    let mut config = Config::default();
    config.grouping.overflow = Overflow::RelatedSuffix;
    let plan = plan(&config, &source, &model)?;
    assert_eq!(plan.findings.len(), 1);
    assert_eq!(
        plan.findings[0].anchor, 2,
        "control statement owns suppression/expectation"
    );
    assert_eq!(
        apply_edits(&source, &plan.edits())?,
        "unit0;\n\n    unit1;\n    unit2;"
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn whole_group_overflow_separates_control() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["ready"]);
    control.header_reads = places(&["ready"]);
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Let, UnitKind::Control],
        &[facts(&["other"], &[]), facts(&["ready"], &[]), control],
        &[0, 0],
    );
    let plan = plan(&Config::default(), &source, &model)?;
    assert_eq!(
        apply_edits(&source, &plan.edits())?,
        "unit0;\n    unit1;\n\n    unit2;"
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn whole_group_default_preserves_complete_setup_groups() -> Result<(), Box<dyn Error>> {
    let mut receiver_call = facts(&[], &["panel"]);
    receiver_call.receivers = places(&["panel"]);
    for (name, kind, setup, inputs) in [
        (
            "related declarations",
            UnitKind::Let,
            vec![
                facts(&["sum"], &["values"]),
                facts(&["difference"], &["values"]),
            ],
            vec!["sum", "difference"],
        ),
        (
            "parallel declarations",
            UnitKind::Let,
            vec![
                facts(&["width"], &["bounds"]),
                facts(&["height"], &["bounds"]),
            ],
            vec!["height"],
        ),
        (
            "four parallel declarations",
            UnitKind::Let,
            vec![
                facts(&["a"], &["events"]),
                facts(&["b"], &["events"]),
                facts(&["c"], &["events"]),
                facts(&["d"], &["events"]),
            ],
            vec!["a", "b", "c", "d"],
        ),
        (
            "direct producer chain",
            UnitKind::Let,
            vec![facts(&["input"], &[]), facts(&["ready"], &["input"])],
            vec!["ready"],
        ),
        (
            "same receiver calls",
            UnitKind::Expression,
            vec![receiver_call.clone(), receiver_call],
            vec!["panel"],
        ),
    ] {
        let count = setup.len();
        let mut semantics = setup;
        let mut control = facts(&[], &inputs);
        control.header_reads = places(&inputs);
        semantics.push(control);
        let mut kinds = vec![kind; count];
        kinds.push(UnitKind::Control);
        let (source, initial_model) = model(&kinds, &semantics, &vec![0; count]);
        let config = Config::default();
        let first = plan(&config, &source, &initial_model)?;
        let mut expected_blanks = vec![0; count];
        expected_blanks[count - 1] = 1;
        let (expected, fixed_model) = model(&kinds, &semantics, &expected_blanks);
        assert_eq!(apply_edits(&source, &first.edits())?, expected, "{name}");
        let repeated = plan(&config, &source, &initial_model)?;
        assert_eq!(apply_edits(&source, &repeated.edits())?, expected, "{name}");
        assert!(
            plan(&config, &expected, &fixed_model)?.edits().is_empty(),
            "{name}"
        );
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn explicit_related_suffix_bounds_a_direct_producer_chain() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["ready"]);
    control.header_reads = places(&["ready"]);
    let semantics = [
        facts(&["input"], &[]),
        facts(&["ready"], &["input"]),
        control,
    ];
    let kinds = [UnitKind::Let, UnitKind::Let, UnitKind::Control];
    let (source, initial_model) = model(&kinds, &semantics, &[0, 0]);
    let mut config = Config::default();
    config.grouping.overflow = Overflow::RelatedSuffix;
    let first = plan(&config, &source, &initial_model)?;
    let (expected, fixed_model) = model(&kinds, &semantics, &[1, 0]);
    assert_eq!(apply_edits(&source, &first.edits())?, expected);
    assert!(plan(&config, &expected, &fixed_model)?.edits().is_empty());
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn mandatory_result_pair_overrides_zero_setup_limit() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["result"]);
    control.header_reads = places(&["result"]);
    control.check_of = Some(Place::local("result"));
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Control],
        &[facts(&["result"], &[]), control],
        &[1],
    );
    let mut c = Config::default();
    c.grouping.max_before_control = 0;
    let plan = plan(&c, &source, &model)?;
    assert_eq!(plan.findings.len(), 1);
    assert_eq!(plan.findings[0].rule, Rule::ResultCheck);
    assert_eq!(apply_edits(&source, &plan.edits())?, "unit0;\n    unit1;");
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn result_pair_does_not_join_across_leading_comment() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["result"]);
    control.header_reads = places(&["result"]);
    control.check_of = Some(Place::local("result"));
    let (source, mut model) = model(
        &[UnitKind::Let, UnitKind::Control],
        &[facts(&["result"], &[]), control],
        &[1],
    );
    model.lists[0].gaps[0].joinable = false;
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn guard_chain_is_an_explicit_exception() -> Result<(), Box<dyn Error>> {
    let (source, mut model) = model(
        &[UnitKind::Control, UnitKind::Control],
        &[facts(&[], &[]), facts(&[], &[])],
        &[0],
    );
    for unit in &mut model.lists[0].units {
        unit.is_guard = true;
    }
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn standalone_block_is_separated() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Block, UnitKind::Expression],
        &[facts(&[], &[]), facts(&[], &[])],
        &[0],
    );
    assert_eq!(
        plan(&Config::default(), &source, &model)?.findings[0].rule,
        Rule::AfterBlock
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn short_tail_stays_compact() -> Result<(), Box<dyn Error>> {
    let (source, mut model) = model(
        &[UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&[], &[])],
        &[0],
    );
    model.lists[0].units[1].is_tail = true;
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn long_tail_after_immediate_producer_stays_compact() -> Result<(), Box<dyn Error>> {
    let (source, mut model) = model(
        &[UnitKind::Let, UnitKind::Let, UnitKind::Expression],
        &[facts(&["a"], &[]), facts(&["b"], &[]), facts(&[], &["b"])],
        &[0, 0],
    );
    model.lists[0].units[2].is_tail = true;
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn always_separate_tail_never_pads_a_sole_expression() -> Result<(), Box<dyn Error>> {
    let (source, mut model) = model(&[UnitKind::Expression], &[facts(&[], &[])], &[]);
    model.lists[0].units[0].is_tail = true;
    let mut c = Config::default();
    c.exits.tail = Tail::AlwaysSeparate;
    assert_eq!(count(&c, &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn functions_separate_and_compact_declarations_do_not() -> Result<(), Box<dyn Error>> {
    for (kind, expected) in [
        (ItemKind::Function, 1),
        (ItemKind::Major, 1),
        (ItemKind::Compact, 0),
    ] {
        let (source, mut model) = model(
            &[UnitKind::Item(kind); 2],
            &[facts(&[], &[]), facts(&[], &[])],
            &[0],
        );
        model.lists[0].item_list = true;
        assert_eq!(count(&Config::default(), &source, &model)?, expected);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn exhaustive_small_policy_models_are_idempotent() -> Result<(), Box<dyn Error>> {
    let kinds = [
        UnitKind::Let,
        UnitKind::Expression,
        UnitKind::Control,
        UnitKind::Exit,
    ];
    for a in kinds {
        for b in kinds {
            for c in kinds {
                for mask in 0..4 {
                    let (source, initial_model) = model(
                        &[a, b, c],
                        &[facts(&[], &[]), facts(&[], &[]), facts(&[], &[])],
                        &[mask & 1, (mask >> 1) & 1],
                    );
                    let mut config = Config::default();
                    config.grouping.expressions = Expressions::Strict;
                    let result = plan(&config, &source, &initial_model);
                    assert!(result.is_ok(), "{a:?} {b:?} {c:?} mask={mask}: {result:?}");
                    let first = result?;
                    let fixed = apply_edits(&source, &first.edits())?;
                    // The synthetic source contains three `unitN;` markers.
                    // Read the two intervening gaps from the applied output.
                    let blanks: Vec<usize> = fixed
                        .split(';')
                        .skip(1)
                        .take(2)
                        .map(|following| {
                            return following
                                .chars()
                                .take_while(|ch| return ch.is_whitespace())
                                .filter(|ch| return *ch == '\n')
                                .count()
                                .saturating_sub(1);
                        })
                        .collect();
                    let (rebuilt, fixed_model) = model(
                        &[a, b, c],
                        &[facts(&[], &[]), facts(&[], &[]), facts(&[], &[])],
                        &blanks,
                    );
                    assert_eq!(rebuilt, fixed);
                    let second = plan(&config, &fixed, &fixed_model)?;
                    assert!(
                        second.edits().is_empty(),
                        "{a:?} {b:?} {c:?} mask={mask}: {second:?}"
                    );
                    assert_eq!(apply_edits(&fixed, &second.edits())?, fixed);
                }
            }
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn mandatory_check_pair_still_separates_unrelated_prefix() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["result"]);
    control.header_reads = places(&["result"]);
    control.check_of = Some(Place::local("result"));
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Let, UnitKind::Control],
        &[facts(&["other"], &[]), facts(&["result"], &[]), control],
        &[0, 1],
    );
    let plan = plan(&Config::default(), &source, &model)?;
    assert_eq!(plan.edits().len(), 2);
    assert_eq!(
        apply_edits(&source, &plan.edits())?,
        "unit0;\n\n    unit1;\n    unit2;"
    );
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn setup_overflow_preserves_entire_mandatory_check_component() -> Result<(), Box<dyn Error>> {
    let mut control = facts(&[], &["result"]);
    control.header_reads = places(&["result"]);
    control.check_of = Some(Place::local("result"));
    let kinds = [
        UnitKind::Let,
        UnitKind::Let,
        UnitKind::Let,
        UnitKind::Control,
    ];
    let semantics = [
        facts(&["other"], &[]),
        facts(&["input"], &[]),
        facts(&["result"], &["input"]),
        control,
    ];
    for overflow in [Overflow::WholeGroup, Overflow::RelatedSuffix] {
        let mut config = Config::default();
        config.grouping.overflow = overflow;
        config.grouping.max_before_control = 0;
        config.grouping.join_related = true;
        let (source, initial_model) = model(&kinds, &semantics, &[0, 1, 1]);
        let first = plan(&config, &source, &initial_model)?;
        let (expected, fixed_model) = model(&kinds, &semantics, &[1, 0, 0]);
        assert_eq!(
            apply_edits(&source, &first.edits())?,
            expected,
            "{overflow:?}"
        );
        assert!(
            plan(&config, &expected, &fixed_model)?.edits().is_empty(),
            "{overflow:?}"
        );
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn empty_drain_loop_attaches_only_to_known_mutation() -> Result<(), Box<dyn Error>> {
    let kinds = [UnitKind::Expression, UnitKind::Control];
    let mut mutation = facts(&[], &["buffer"]);
    mutation.mutating_receivers = places(&["buffer"]);
    let mut assignment = facts(&[], &["buffer"]);
    assignment.writes = places(&["buffer"]);
    for (name, previous, compact) in [
        ("mutating receiver", mutation, true),
        ("assignment", assignment, true),
        ("unrelated call", facts(&[], &["buffer"]), false),
        ("unknown call", Facts::default(), false),
    ] {
        let (source, mut initial_model) = model(&kinds, &[previous, facts(&[], &["events"])], &[0]);
        initial_model.lists[0].units[1].is_empty_loop = true;
        let mut config = Config::default();
        config.grouping.max_before_control = 0;
        let first = plan(&config, &source, &initial_model)?;
        let expected = if compact {
            "unit0;\n    unit1;"
        } else {
            "unit0;\n\n    unit1;"
        };
        assert_eq!(apply_edits(&source, &first.edits())?, expected, "{name}");
        for strict_expressions in [false, true] {
            let mut strict = config.clone();
            if strict_expressions {
                strict.grouping.expressions = Expressions::Strict;
            } else {
                strict.control_flow.compact_cleanup = false;
            }
            assert_eq!(
                apply_edits(&source, &plan(&strict, &source, &initial_model)?.edits())?,
                "unit0;\n\n    unit1;",
                "{name}, strict expressions: {strict_expressions}"
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
fn receiver_centered_setup_can_attach_to_control() -> Result<(), Box<dyn Error>> {
    let mut setup = facts(&[], &["buffer"]);
    setup.receivers = places(&["buffer"]);
    let mut control = facts(&[], &["buffer"]);
    control.header_reads = places(&["buffer"]);
    let (source, model) = model(
        &[UnitKind::Expression, UnitKind::Control],
        &[setup, control],
        &[0],
    );
    assert_eq!(count(&Config::default(), &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn compact_declaration_setting_does_not_override_function_preserve() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Item(ItemKind::Function); 2],
        &[facts(&[], &[]), facts(&[], &[])],
        &[0],
    );
    let mut config = Config::default();
    config.items.functions = Separation::Preserve;
    config.items.compact_declarations = false;
    assert_eq!(count(&config, &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn after_block_preserve_allows_adjacent_controls() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Control; 2],
        &[facts(&[], &["a"]), facts(&[], &["b"])],
        &[0],
    );
    let mut config = Config::default();
    assert_eq!(count(&config, &source, &model)?, 1);
    config.control_flow.after_block = Separation::Preserve;
    assert_eq!(count(&config, &source, &model)?, 0);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn after_block_receiver_continuation_honors_identity_and_policy() -> Result<(), Box<dyn Error>> {
    let mut block = facts(&[], &["panel"]);
    block.receivers = places(&["panel"]);
    for (name, receiver, kind, enabled, compact) in [
        ("same receiver", "panel", UnitKind::Expression, true, true),
        (
            "different receiver",
            "other",
            UnitKind::Expression,
            true,
            false,
        ),
        ("disabled", "panel", UnitKind::Expression, false, false),
        ("following control", "panel", UnitKind::Control, true, false),
    ] {
        let mut next = facts(&[], &[receiver]);
        next.receivers = places(&[receiver]);
        let (source, initial_model) =
            model(&[UnitKind::Control, kind], &[block.clone(), next], &[0]);
        let mut config = Config::default();
        config.control_flow.related_continuation = enabled;
        let result = plan(&config, &source, &initial_model)?;
        if compact {
            assert_eq!(apply_edits(&source, &result.edits())?, source, "{name}");
        } else {
            assert_eq!(
                apply_edits(&source, &result.edits())?,
                "unit0;\n\n    unit1;",
                "{name}"
            );
            assert_eq!(result.findings[0].rule, Rule::AfterBlock, "{name}");
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn header_input_proves_continuation_despite_unknown_body() -> Result<(), Box<dyn Error>> {
    for (receiver, shared_inputs, expected) in [
        ("panel", true, "unit0;\n    unit1;"),
        ("other", true, "unit0;\n\n    unit1;"),
        ("panel", false, "unit0;\n\n    unit1;"),
    ] {
        let mut block = facts(&[], &[]);
        block.known = false;
        block.header_reads = places(&["panel"]);
        let mut next = facts(&[], &[receiver]);
        next.receivers = places(&[receiver]);
        let (source, initial_model) = model(
            &[UnitKind::Control, UnitKind::Expression],
            &[block, next],
            &[0],
        );
        let mut config = Config::default();
        config.grouping.shared_inputs = shared_inputs;
        let result = plan(&config, &source, &initial_model)?;
        assert_eq!(apply_edits(&source, &result.edits())?, expected);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn empty_drain_chain_and_bare_return_honor_cleanup_policy() -> Result<(), Box<dyn Error>> {
    let kinds = [UnitKind::Control, UnitKind::Control, UnitKind::Exit];
    let semantics = [
        facts(&[], &["first"]),
        facts(&[], &["second"]),
        facts(&[], &[]),
    ];
    let (source, mut initial_model) = model(&kinds, &semantics, &[0, 0]);
    initial_model.lists[0].units[0].is_empty_loop = true;
    initial_model.lists[0].units[1].is_empty_loop = true;
    initial_model.lists[0].units[2].is_bare_return = true;
    let config = Config::default();
    assert_eq!(
        apply_edits(&source, &plan(&config, &source, &initial_model)?.edits())?,
        source
    );
    for strict_expressions in [false, true] {
        let mut strict = Config::default();
        if strict_expressions {
            strict.grouping.expressions = Expressions::Strict;
        } else {
            strict.control_flow.compact_cleanup = false;
        }
        assert_eq!(
            apply_edits(&source, &plan(&strict, &source, &initial_model)?.edits())?,
            "unit0;\n\n    unit1;\n\n    unit2;",
            "strict expressions: {strict_expressions}"
        );
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn valueless_loop_exit_attaches_to_assignment_in_long_block() -> Result<(), Box<dyn Error>> {
    let mut assignment = facts(&[], &["state"]);
    assignment.writes = places(&["state"]);
    let (source, mut initial_model) = model(
        &[UnitKind::Expression, UnitKind::Exit],
        &[assignment, facts(&[], &[])],
        &[0],
    );
    initial_model.lists[0].units[1].is_loop_exit = true;
    initial_model.lists[0].executable_count = 5;
    let mut config = Config::default();
    assert_eq!(
        apply_edits(&source, &plan(&config, &source, &initial_model)?.edits())?,
        source
    );
    config.exits.attached_loop_exit = false;
    let separated = plan(&config, &source, &initial_model)?;
    assert_eq!(
        apply_edits(&source, &separated.edits())?,
        "unit0;\n\n    unit1;"
    );
    assert_eq!(separated.findings[0].rule, Rule::Exit);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn receiver_group_bridges_consumed_setup_only() -> Result<(), Box<dyn Error>> {
    for (input, blanks, compact) in [
        ("buffer", [0, 0, 0], true),
        ("other", [0, 0, 0], false),
        ("buffer", [1, 0, 0], true),
    ] {
        let mut receiver = facts(&[], &["panel"]);
        receiver.receivers = places(&["panel"]);
        let mut control = facts(&[], &["panel", input]);
        control.receivers = places(&["panel"]);
        control.header_reads = places(&["panel", input]);
        let (source, initial) = model(
            &[
                UnitKind::Expression,
                UnitKind::Let,
                UnitKind::Control,
                UnitKind::Expression,
            ],
            &[
                receiver.clone(),
                facts(&["buffer"], &["source"]),
                control,
                receiver,
            ],
            &blanks,
        );
        let result = plan(&Config::default(), &source, &initial)?;
        let output = apply_edits(&source, &result.edits())?;
        if compact {
            assert_eq!(output, source);
        } else {
            assert!(output.contains("unit0;\n\n    unit1;"));
        }
        for strict in [false, true] {
            let mut config = Config::default();
            if strict {
                config.grouping.expressions = Expressions::Strict;
            } else {
                config.grouping.max_before_control = 0;
            }
            let result = plan(&config, &source, &initial)?;
            let boundary = if strict {
                "unit0;\n\n    unit1;"
            } else {
                "unit1;\n\n    unit2;"
            };
            assert!(apply_edits(&source, &result.edits())?.contains(boundary));
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn receiver_group_keeps_local_flag_check_with_closing_operation() -> Result<(), Box<dyn Error>> {
    for (flag, compact) in [("flag", true), ("unrelated", false)] {
        let mut receiver = facts(&[], &["panel"]);
        receiver.receivers = places(&["panel"]);
        let mut deferred = receiver.clone();
        deferred.known = false;
        let mut control = facts(&[], &[flag]);
        control.header_reads = places(&[flag]);
        control.writes = places(&["changed"]);
        let (source, initial) = model(
            &[
                UnitKind::Expression,
                UnitKind::Let,
                UnitKind::Expression,
                UnitKind::Control,
                UnitKind::Expression,
            ],
            &[
                receiver.clone(),
                facts(&["flag"], &[]),
                deferred,
                control,
                receiver,
            ],
            &[0, 0, 0, 0],
        );
        let result = plan(&Config::default(), &source, &initial)?;
        let output = apply_edits(&source, &result.edits())?;
        if compact {
            assert_eq!(output, source);
        } else {
            assert!(output.contains("unit3;\n\n    unit4;"));
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn explicit_return_honors_producer_and_tail_policy() -> Result<(), Box<dyn Error>> {
    for (input, tail, separated) in [
        ("result", Tail::Smart, false),
        ("other", Tail::Smart, true),
        ("result", Tail::AlwaysSeparate, true),
        ("other", Tail::Preserve, false),
    ] {
        let (source, initial) = model(
            &[UnitKind::Let, UnitKind::Let, UnitKind::Exit],
            &[
                facts(&["first"], &[]),
                facts(&["result"], &[]),
                facts(&[], &[input]),
            ],
            &[0, 0],
        );
        let mut config = Config::default();
        config.exits.short_block_max_statements = 2;
        config.exits.tail = tail;
        let result = plan(&config, &source, &initial)?;
        let output = apply_edits(&source, &result.edits())?;
        assert_eq!(output.contains("unit1;\n\n    unit2;"), separated);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn plain_mutation_cleanup_honors_explicit_policy() -> Result<(), Box<dyn Error>> {
    for (mutates, enabled, tail, separated) in [
        (true, true, Tail::Smart, false),
        (false, true, Tail::Smart, true),
        (true, false, Tail::Smart, true),
        (true, true, Tail::AlwaysSeparate, true),
    ] {
        let mut operation = facts(&[], &["state"]);
        if mutates {
            operation.mutating_receivers = places(&["state"]);
        }
        let (source, mut initial) = model(
            &[UnitKind::Expression, UnitKind::Expression, UnitKind::Exit],
            &[operation.clone(), operation, facts(&[], &[])],
            &[0, 0],
        );
        initial.lists[0].units[2].is_bare_return = true;
        let mut config = Config::default();
        config.exits.short_block_max_statements = 2;
        config.control_flow.compact_cleanup = enabled;
        config.exits.tail = tail;
        let result = plan(&config, &source, &initial)?;
        let output = apply_edits(&source, &result.edits())?;
        assert_eq!(output.contains("unit1;\n\n    unit2;"), separated);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn receiver_group_and_tail_constraints_remain_idempotent() -> Result<(), Box<dyn Error>> {
    let mut receiver = facts(&[], &["panel"]);
    receiver.receivers = places(&["panel"]);
    let mut control = facts(&[], &["panel", "buffer"]);
    control.known = false;
    control.header_reads = places(&["panel", "buffer"]);
    let (source, mut initial) = model(
        &[UnitKind::Expression, UnitKind::Let, UnitKind::Control],
        &[receiver, facts(&["buffer"], &["source"]), control],
        &[0, 0],
    );
    initial.lists[0].units[2].is_tail = true;
    for (tail, expected) in [
        (Tail::Smart, source.as_str()),
        (Tail::AlwaysSeparate, "unit0;\n\n    unit1;\n\n    unit2;"),
    ] {
        let mut config = Config::default();
        config.exits.tail = tail;
        let result = plan(&config, &source, &initial)?;
        assert_eq!(apply_edits(&source, &result.edits())?, expected);
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn completion_exits_preserve_spacing_but_honor_explicit_separation() -> Result<(), Box<dyn Error>> {
    for implicit in [false, true] {
        for known in [false, true] {
            for tail in [Tail::Smart, Tail::AlwaysSeparate, Tail::Preserve] {
                let mut completion = facts(&[], &[]);
                completion.known = known;
                let (source, mut initial) = model(
                    &[
                        UnitKind::Let,
                        UnitKind::Let,
                        if implicit {
                            UnitKind::Expression
                        } else {
                            UnitKind::Exit
                        },
                    ],
                    &[facts(&["a"], &[]), facts(&["b"], &[]), completion],
                    &[0, 0],
                );
                initial.lists[0].units[2].is_tail = implicit;
                let mut config = Config::only(&[Rule::Exit]);
                config.exits.short_block_max_statements = 2;
                config.exits.tail = tail;
                assert_eq!(
                    count(&config, &source, &initial)?,
                    usize::from(tail == Tail::AlwaysSeparate || (tail == Tail::Smart && !known))
                );
                initial.lists[0].gaps[1].blank_lines = 1;
                assert_eq!(count(&config, &source, &initial)?, 0);
            }
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn block_data_continuations_respect_identity_and_opt_outs() -> Result<(), Box<dyn Error>> {
    for header in [false, true] {
        for next_kind in [
            UnitKind::Control,
            UnitKind::Let,
            UnitKind::Assignment,
            UnitKind::Exit,
        ] {
            for related in [false, true] {
                for enabled in [false, true] {
                    let mut block = facts(&[], &[]);
                    if header {
                        block.header_reads = places(&["state"]);
                    } else {
                        block.writes = places(&["state"]);
                    }
                    let next = facts(&[], &[if related { "state" } else { "other" }]);
                    let (source, initial) =
                        model(&[UnitKind::Control, next_kind], &[block, next], &[0]);
                    let mut config = Config::only(&[Rule::AfterBlock]);
                    config.control_flow.related_continuation = enabled;
                    assert_eq!(
                        count(&config, &source, &initial)?,
                        usize::from(!(related && enabled))
                    );
                    config.grouping.expressions = Expressions::Strict;
                    assert_eq!(count(&config, &source, &initial)?, 1);
                }
            }
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn guard_and_final_return_share_a_phase() -> Result<(), Box<dyn Error>> {
    let (source, mut initial) = model(
        &[UnitKind::Let, UnitKind::Control, UnitKind::Exit],
        &[
            facts(&["setup"], &[]),
            facts(&[], &["condition"]),
            facts(&[], &["result"]),
        ],
        &[1, 0],
    );
    initial.lists[0].units[1].is_guard = true;
    let mut config = Config::only(&[Rule::Exit, Rule::AfterBlock]);
    assert_eq!(count(&config, &source, &initial)?, 0);
    config.exits.tail = Tail::AlwaysSeparate;
    assert_eq!(count(&config, &source, &initial)?, 1);
    config.exits.tail = Tail::Smart;
    config.control_flow.guard_chain = GuardChain::Separate;
    assert_eq!(count(&config, &source, &initial)?, 1);
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn mutating_methods_produce_returned_values_even_in_long_blocks() -> Result<(), Box<dyn Error>> {
    for mutable in [false, true] {
        for returned in ["buffer", "other"] {
            let mut operation = facts(&[], &["buffer"]);
            operation.receivers = places(&["buffer"]);
            if mutable {
                operation.mutating_receivers = places(&["buffer"]);
            }
            let (source, initial) = model(
                &[
                    UnitKind::Let,
                    UnitKind::Let,
                    UnitKind::Let,
                    UnitKind::Expression,
                    UnitKind::Exit,
                ],
                &[
                    facts(&["a"], &[]),
                    facts(&["b"], &[]),
                    facts(&["buffer"], &[]),
                    operation,
                    facts(&[], &[returned]),
                ],
                &[0, 0, 0, 0],
            );
            let mut config = Config::only(&[Rule::Exit]);
            assert_eq!(
                count(&config, &source, &initial)?,
                usize::from(!(mutable && returned == "buffer"))
            );
            config.exits.tail = Tail::AlwaysSeparate;
            assert_eq!(count(&config, &source, &initial)?, 1);
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn short_scope_returns_respect_threshold_and_tail_policy() -> Result<(), Box<dyn Error>> {
    for length in [3, 4, 5] {
        for implicit in [false, true] {
            let mut kinds = vec![UnitKind::Let; length];
            kinds[length - 1] = if implicit {
                UnitKind::Expression
            } else {
                UnitKind::Exit
            };
            let mut semantics = vec![facts(&["unused"], &[]); length];
            semantics[0] = facts(&["path"], &[]);
            semantics[length - 1] = facts(&[], &["path"]);
            let (source, mut initial) = model(&kinds, &semantics, &vec![0; length - 1]);
            initial.lists[0].units[length - 1].is_tail = implicit;
            let mut config = Config::only(&[Rule::Exit]);
            assert_eq!(count(&config, &source, &initial)?, usize::from(length > 4));
            config.exits.short_block_max_statements = 2;
            assert_eq!(count(&config, &source, &initial)?, 1);
            config.exits.tail = Tail::Preserve;
            assert_eq!(count(&config, &source, &initial)?, 0);
            config.exits.short_block_max_statements = 4;
            config.exits.tail = Tail::AlwaysSeparate;
            assert_eq!(count(&config, &source, &initial)?, 1);
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn caching_a_value_stays_with_its_return_even_with_an_unknown_write_target()
-> Result<(), Box<dyn Error>> {
    for known in [false, true] {
        for input in ["state", "other"] {
            for implicit in [false, true] {
                let mut cache = facts(&[], &["state"]);
                cache.known = known;
                let (source, mut initial) = model(
                    &[
                        UnitKind::Let,
                        UnitKind::Assignment,
                        if implicit {
                            UnitKind::Expression
                        } else {
                            UnitKind::Exit
                        },
                    ],
                    &[facts(&["state"], &[]), cache, facts(&[], &[input])],
                    &[1, 0],
                );
                initial.lists[0].units[2].is_tail = implicit;
                let mut config = Config::only(&[Rule::Exit]);
                config.exits.short_block_max_statements = 2;
                assert_eq!(
                    count(&config, &source, &initial)?,
                    usize::from(input != "state")
                );
                config.grouping.shared_inputs = false;
                assert_eq!(count(&config, &source, &initial)?, 1);
                config.grouping.shared_inputs = true;
                config.exits.tail = Tail::AlwaysSeparate;
                assert_eq!(count(&config, &source, &initial)?, 1);
                config.exits.tail = Tail::Smart;
                config.grouping.expressions = Expressions::Strict;
                assert_eq!(count(&config, &source, &initial)?, 1);
            }
        }
    }
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary"
)]
fn completion_after_a_loop_honors_explicit_policy() -> Result<(), Box<dyn Error>> {
    for implicit in [false, true] {
        let (source, mut initial) = model(
            &[
                UnitKind::Control,
                if implicit {
                    UnitKind::Expression
                } else {
                    UnitKind::Exit
                },
            ],
            &[Facts::default(), facts(&[], &[])],
            &[0],
        );
        initial.lists[0].units[1].is_tail = implicit;
        let mut config = Config::only(&[Rule::Exit, Rule::AfterBlock]);
        config.exits.short_block_max_statements = 0;
        assert_eq!(count(&config, &source, &initial)?, 0);
        config.exits.tail = Tail::AlwaysSeparate;
        assert_eq!(count(&config, &source, &initial)?, 1);
        config.exits.tail = Tail::Smart;
        config.grouping.expressions = Expressions::Strict;
        assert_eq!(count(&config, &source, &initial)?, 1);
    }
    return Ok(());
}
