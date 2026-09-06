//! Behavioral regression tests for spacing policy.

use std::collections::BTreeSet;
use std::error::Error;

use rust_statement_spacing_core::config::{Expressions, Overflow, Separation, Tail};
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
    let plan = plan(&Config::default(), &source, &model)?;
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
    let mut c = Config::default();
    c.grouping.overflow = Overflow::WholeGroup;
    let plan = plan(&c, &source, &model)?;
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
fn explicit_exit_in_long_block_is_separated() -> Result<(), Box<dyn Error>> {
    let (source, model) = model(
        &[UnitKind::Let, UnitKind::Let, UnitKind::Exit],
        &[facts(&["a"], &[]), facts(&["b"], &[]), facts(&[], &["b"])],
        &[0, 0],
    );
    assert_eq!(
        plan(&Config::default(), &source, &model)?.findings[0].rule,
        Rule::Exit
    );
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
