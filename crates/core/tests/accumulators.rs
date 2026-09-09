//! Bounded control setup relationships from non-deferred mutation facts.

use rust_statement_spacing_core::config::{Expressions, Overflow, UseIn};
use rust_statement_spacing_core::*;
use std::collections::BTreeSet;
use std::error::Error;
use std::iter::once;
use std::slice::from_ref;

fn places(ids: &[&str]) -> BTreeSet<Place> {
    return ids.iter().map(|id| return Place::local(*id)).collect();
}

fn initialized(id: &str) -> Facts {
    return Facts {
        known: true,
        definitions: places(&[id]),
        ..Facts::default()
    };
}

fn updates(id: &str) -> Facts {
    return Facts {
        known: true,
        writes: places(&[id]),
        ..Facts::default()
    };
}

// Compiler identities and effect scopes are supplied at the public planner seam;
// real-source coverage of nested and deferred bodies belongs to compiler fixtures.
fn rendered(
    config: &Config,
    setup: &[Facts],
    control: &Facts,
    blank_lines: usize,
) -> Result<String, String> {
    let mut source = String::new();
    let mut units = Vec::new();
    let mut gaps = Vec::new();
    for (index, facts) in setup.iter().chain(once(control)).enumerate() {
        if index != 0 {
            let start = source.len();
            for _ in 0..blank_lines {
                source.push('\n');
            }

            source.push_str("\n    ");
            gaps.push(Gap {
                range: ByteRange::new(start, source.len()),
                blank_lines,
                vertical: true,
                protected: false,
                joinable: true,
            });
        }

        let is_control = index == setup.len();
        let start = source.len();
        source.push_str(if is_control { "control;" } else { "setup;" });
        let range = ByteRange::new(start, source.len());
        units.push(Unit {
            range,
            code_range: range,
            shape: Shape {
                form: if is_control { Form::Loop } else { Form::Call },
                ..Default::default()
            },
            kind: if is_control {
                UnitKind::Control
            } else {
                UnitKind::Let
            },
            is_tail: false,
            is_guard: false,
            is_empty_loop: false,
            is_loop_exit: false,
            is_bare_return: false,
            active: true,
            protected: false,
            facts: facts.clone(),
            enabled: RuleMask::all(),
            anchor: index,
        });
    }

    let model = SourceModel {
        lists: vec![UnitList {
            executable_count: units.len(),
            units,
            gaps,
            item_list: false,
            scope: Default::default(),
        }],
        layout_edges: vec![],
    };

    return apply_edits(&source, &plan(config, &source, &model)?.edits());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn nested_updates_attach_without_first_body_reads() -> Result<(), Box<dyn Error>> {
    let assignment = updates("outer:state");
    let mutable_call = Facts {
        known: true,
        mutating_receivers: places(&["outer:state"]),
        ..Facts::default()
    };

    for control in [assignment, mutable_call] {
        assert_eq!(
            rendered(
                &Config::default(),
                &[initialized("outer:state")],
                &control,
                0
            )?,
            "setup;\n    control;"
        );
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn late_reads_and_other_bindings_do_not_prove_accumulation() -> Result<(), Box<dyn Error>> {
    for (case, control) in [
        ("unrelated write", updates("other:state")),
        ("shadowed binding", updates("inner:state")),
        (
            "late read only",
            Facts {
                known: true,
                reads: places(&["outer:state"]),
                whole_body_reads: places(&["outer:state"]),
                ..Facts::default()
            },
        ),
        (
            "unproven mutable receiver",
            Facts {
                known: true,
                receivers: places(&["outer:state"]),
                ..Facts::default()
            },
        ),
        (
            "deferred mutation excluded by compiler",
            Facts {
                known: true,
                ..Facts::default()
            },
        ),
    ] {
        assert_eq!(
            rendered(
                &Config::default(),
                &[initialized("outer:state")],
                &control,
                0
            )?,
            "setup;\n\n    control;",
            "{case}"
        );
    }

    let receiver_only_setup = Facts {
        known: true,
        receivers: places(&["outer:state"]),
        ..Facts::default()
    };
    assert_eq!(
        rendered(
            &Config::default(),
            &[receiver_only_setup],
            &updates("outer:state"),
            0,
        )?,
        "setup;\n\n    control;"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn accumulator_respects_explicit_scope_and_zero_limit() -> Result<(), Box<dyn Error>> {
    let mut header = Config::default();
    header.grouping.use_in = UseIn::Header;
    let mut strict = Config::default();
    strict.grouping.expressions = Expressions::Strict;
    let mut zero = Config::default();
    zero.grouping.max_before_control = 0;

    for config in [header, strict, zero] {
        assert_eq!(
            rendered(&config, &[initialized("state")], &updates("state"), 0)?,
            "setup;\n\n    control;"
        );
    }

    let mut whole_body = Config::default();
    whole_body.grouping.use_in = UseIn::WholeBody;
    let read_only = Facts {
        known: true,
        reads: places(&["state"]),
        whole_body_reads: places(&["state"]),
        ..Facts::default()
    };
    assert_eq!(
        rendered(&whole_body, &[initialized("state")], &read_only, 0)?,
        "setup;\n    control;"
    );

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn accumulator_remains_subject_to_group_overflow() -> Result<(), Box<dyn Error>> {
    let mut both = updates("state");
    both.writes.insert(Place::local("other"));
    let setup = [initialized("other"), initialized("state")];
    let mut larger = Config::default();
    larger.grouping.max_before_control = 2;
    let mut suffix = Config::default();
    suffix.grouping.overflow = Overflow::RelatedSuffix;
    for (config, control, expected) in [
        (
            Config::default(),
            both.clone(),
            "setup;\n\n    setup;\n    control;",
        ),
        (larger.clone(), both, "setup;\n    setup;\n    control;"),
        (
            larger,
            updates("state"),
            "setup;\n\n    setup;\n    control;",
        ),
        (
            suffix,
            updates("state"),
            "setup;\n\n    setup;\n    control;",
        ),
    ] {
        assert_eq!(rendered(&config, &setup, &control, 0)?, expected);
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn unknown_facts_remain_conservative_but_do_not_override_zero_limit() -> Result<(), Box<dyn Error>>
{
    for (setup, control) in [
        (Facts::default(), updates("state")),
        (initialized("state"), Facts::default()),
    ] {
        assert_eq!(
            rendered(&Config::default(), from_ref(&setup), &control, 0)?,
            "setup;\n    control;"
        );
        let mut zero = Config::default();
        zero.grouping.max_before_control = 0;
        assert_eq!(
            rendered(&zero, &[setup], &control, 0)?,
            "setup;\n\n    control;"
        );
    }

    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn opaque_body_preserves_proven_population_without_inventing_relationships()
-> Result<(), Box<dyn Error>> {
    let mut config = Config::default();
    config.grouping.join_related = true;
    assert_eq!(
        rendered(&config, &[initialized("state")], &updates("state"), 1)?,
        "setup;\n    control;"
    );

    let population = Facts {
        known: false,
        mutating_receivers: places(&["state"]),
        ..Facts::default()
    };
    assert_eq!(
        rendered(&config, &[initialized("state")], &population, 1)?,
        "setup;\n    control;"
    );
    assert_eq!(
        rendered(
            &config,
            &[initialized("unrelated"), initialized("state")],
            &population,
            0,
        )?,
        "setup;\n\n    setup;\n    control;"
    );

    for control in [
        Facts::default(),
        Facts {
            known: false,
            reads: places(&["state"]),
            first_body_reads: places(&["state"]),
            receivers: places(&["state"]),
            mutating_receivers: places(&["other"]),
            ..Facts::default()
        },
    ] {
        assert_eq!(
            rendered(&config, &[initialized("state")], &control, 1)?,
            "setup;\n\n    control;"
        );
    }

    for (use_in, limit, join_related) in [
        (UseIn::Header, 1, true),
        (UseIn::HeaderOrFirstBodyStatement, 0, true),
        (UseIn::HeaderOrFirstBodyStatement, 1, false),
    ] {
        config.grouping.use_in = use_in;
        config.grouping.max_before_control = limit;
        config.grouping.join_related = join_related;
        assert_eq!(
            rendered(&config, &[initialized("state")], &population, 1)?,
            "setup;\n\n    control;"
        );
    }

    return Ok(());
}
