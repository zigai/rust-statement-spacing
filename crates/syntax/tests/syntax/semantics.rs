use std::error::Error;

use super::*;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn no_compiler_mapping_means_no_edits() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    one();\n    two();\n}\n";
    let parsed = parse_source(source, "2024")?;
    assert!(plan(&strict(), source, &parsed.model)?.findings.is_empty());
    return Ok(());
}

#[test]
fn projection_prunes_incidental_base_reads() {
    let index = SemanticIndex {
        anchors: vec![],
        events: vec![
            Event {
                range: ByteRange::new(0, 4),
                kind: EventKind::Read,
                place: Some(Place::local("self")),
            },
            Event {
                range: ByteRange::new(0, 10),
                kind: EventKind::Read,
                place: Some(Place {
                    local: "self".into(),
                    projections: vec![".cache".into()],
                    is_self: true,
                }),
            },
        ],
    };
    let facts = index.facts(ByteRange::new(0, 12), None, None, &[], &[], false);
    assert_eq!(facts.reads.len(), 1);
    assert!(!facts.reads.contains(&Place::local("self")));
}

#[test]
fn deferred_body_use_does_not_attach_outer_control() {
    let captured = Place::local("captured");
    let immediate = Place::local("immediate");
    let mut index = SemanticIndex {
        anchors: vec![],
        events: vec![Event {
            range: ByteRange::new(2, 7),
            kind: EventKind::Read,
            place: Some(immediate.clone()),
        }],
    };
    for kind in [
        EventKind::Read,
        EventKind::Write,
        EventKind::Receiver,
        EventKind::MutatingReceiver,
        EventKind::Inspect,
    ] {
        index.events.push(Event {
            range: ByteRange::new(10, 18),
            kind,
            place: Some(captured.clone()),
        });
    }
    let facts = index.facts(
        ByteRange::new(0, 30),
        None,
        Some(ByteRange::new(0, 30)),
        &[ByteRange::new(0, 30)],
        &[ByteRange::new(8, 25)],
        true,
    );
    assert!(!facts.known);
    assert_eq!(facts.captures, [captured].into());
    assert_eq!(facts.reads, [immediate.clone()].into());
    assert_eq!(facts.header_reads, [immediate.clone()].into());
    assert_eq!(facts.first_body_reads, [immediate.clone()].into());
    assert_eq!(facts.whole_body_reads, [immediate].into());
    assert!(facts.writes.is_empty());
    assert!(facts.receivers.is_empty());
    assert!(facts.mutating_receivers.is_empty());
    assert!(facts.check_of.is_none());
}

#[test]
fn deferred_local_projection_is_not_an_outer_capture() {
    let outer = Place::local("outer-id");
    let deferred_local = Place::local("deferred-local-id");
    let index = SemanticIndex {
        anchors: vec![],
        events: vec![
            Event {
                range: ByteRange::new(10, 15),
                kind: EventKind::Define,
                place: Some(deferred_local.clone()),
            },
            Event {
                range: ByteRange::new(18, 23),
                kind: EventKind::Read,
                place: Some(outer.clone()),
            },
            Event {
                range: ByteRange::new(25, 30),
                kind: EventKind::Read,
                place: Some(deferred_local.clone()),
            },
            Event {
                range: ByteRange::new(25, 36),
                kind: EventKind::Write,
                place: Some(Place {
                    local: deferred_local.local,
                    projections: vec![".field".into()],
                    is_self: false,
                }),
            },
        ],
    };
    let facts = index.facts(
        ByteRange::new(0, 40),
        Some(ByteRange::new(0, 40)),
        None,
        &[],
        &[ByteRange::new(8, 38)],
        true,
    );
    assert_eq!(facts.captures, [outer].into());
    assert!(facts.definitions.is_empty());
    assert!(facts.reads.is_empty());
    assert!(facts.writes.is_empty());
}

#[test]
fn nested_deferral_captures_are_relative_to_the_enclosing_body() {
    let outer = Place::local("outer-id");
    let middle = Place::local("middle-id");
    let inner = Place::local("inner-id");
    let index = SemanticIndex {
        anchors: vec![],
        events: vec![
            Event {
                range: ByteRange::new(10, 16),
                kind: EventKind::Define,
                place: Some(middle.clone()),
            },
            Event {
                range: ByteRange::new(32, 37),
                kind: EventKind::Define,
                place: Some(inner.clone()),
            },
            Event {
                range: ByteRange::new(40, 45),
                kind: EventKind::Read,
                place: Some(outer.clone()),
            },
            Event {
                range: ByteRange::new(47, 53),
                kind: EventKind::Read,
                place: Some(middle.clone()),
            },
            Event {
                range: ByteRange::new(55, 60),
                kind: EventKind::Read,
                place: Some(inner),
            },
        ],
    };
    let outer_facts = index.facts(
        ByteRange::new(0, 80),
        None,
        None,
        &[],
        &[ByteRange::new(8, 75), ByteRange::new(30, 65)],
        true,
    );
    let middle_facts = index.facts(
        ByteRange::new(25, 70),
        None,
        None,
        &[],
        &[ByteRange::new(30, 65)],
        true,
    );
    assert_eq!(outer_facts.captures, [outer.clone()].into());
    assert_eq!(middle_facts.captures, [outer, middle].into());
    assert!(outer_facts.reads.is_empty());
    assert!(middle_facts.reads.is_empty());
}

#[test]
fn direct_callee_ignores_nested_and_deferred_operations() {
    let index = SemanticIndex {
        anchors: Vec::new(),
        events: vec![
            Event {
                range: ByteRange::new(0, 29),
                kind: EventKind::DirectCallee("outer".into()),
                place: None,
            },
            Event {
                range: ByteRange::new(6, 15),
                kind: EventKind::DirectCallee("argument".into()),
                place: None,
            },
            Event {
                range: ByteRange::new(19, 27),
                kind: EventKind::DirectCallee("deferred".into()),
                place: None,
            },
        ],
    };
    let facts = index.facts(
        ByteRange::new(0, 30),
        None,
        None,
        &[],
        &[ByteRange::new(18, 28)],
        false,
    );
    assert_eq!(facts.direct_callees, ["outer".into()].into());
    let deferred = index.facts(
        ByteRange::new(19, 28),
        None,
        None,
        &[],
        &[ByteRange::new(18, 28)],
        false,
    );
    assert!(deferred.direct_callees.is_empty());
}

#[test]
fn enclosing_expression_does_not_inherit_an_inner_callee() {
    let index = SemanticIndex {
        anchors: Vec::new(),
        events: vec![Event {
            range: ByteRange::new(0, 10),
            kind: EventKind::DirectCallee("nested".into()),
            place: None,
        }],
    };
    let facts = index.facts(ByteRange::new(0, 20), None, None, &[], &[], false);
    assert!(facts.direct_callees.is_empty());
}

#[test]
fn mutable_receiver_is_not_an_assignment_write() {
    let receiver = Place::local("collection");
    let index = SemanticIndex {
        anchors: Vec::new(),
        events: vec![Event {
            range: ByteRange::new(0, 10),
            kind: EventKind::MutatingReceiver,
            place: Some(receiver.clone()),
        }],
    };
    let facts = index.facts(ByteRange::new(0, 20), None, None, &[], &[], false);
    assert!(facts.writes.is_empty());
    assert_eq!(facts.mutating_receivers, [receiver].into());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn inactive_sibling_is_a_barrier_not_a_deleted_unit() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    one();\n    #[cfg(any())]\n    two();\n    three();\n}\n";
    let mut parsed = parse_source(source, "2024")?;
    let mut semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
    let disabled_start = source
        .find("two();")
        .ok_or("expected token in source fixture")?;
    semantics
        .anchors
        .retain(|anchor| return anchor.range.start != disabled_start);
    parsed.attach(source, &semantics);
    assert!(plan(&strict(), source, &parsed.model)?.edits().is_empty());
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn block_valued_condition_is_part_of_the_header() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    if { ready } {\n        action();\n    }\n}\n";
    let mut parsed = parse_source(source, "2024")?;
    let mut semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
    let start = source
        .find("ready")
        .ok_or("expected token in source fixture")?;
    let ready = Place::local("ready-id");
    semantics.events.push(Event {
        range: ByteRange::new(start, start + 5),
        kind: EventKind::Read,
        place: Some(ready.clone()),
    });
    parsed.attach(source, &semantics);
    let control = parsed
        .model
        .lists
        .iter()
        .flat_map(|list| return &list.units)
        .find(|unit| return unit.kind == UnitKind::Control)
        .ok_or("expected control-flow unit in parsed fixture")?;
    assert!(control.facts.header_reads.contains(&ready));
    return Ok(());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions define the test failure boundary and Result propagates setup errors"
)]
fn first_match_arm_operation_does_not_include_nested_body_reads() -> Result<(), Box<dyn Error>> {
    let source = "fn f() {\n    match tag {\n        _ => {\n            if ready {\n                use_value(value);\n            }\n        }\n    }\n}\n";
    let mut parsed = parse_source(source, "2024")?;
    let mut semantics = SemanticIndex::structural_anchors(parsed.code_ranges());
    let start = source
        .find("value);")
        .ok_or("expected token in source fixture")?;
    let value = Place::local("value-id");
    semantics.events.push(Event {
        range: ByteRange::new(start, start + 5),
        kind: EventKind::Read,
        place: Some(value.clone()),
    });
    parsed.attach(source, &semantics);
    let control = parsed
        .model
        .lists
        .iter()
        .flat_map(|list| return &list.units)
        .find(|unit| {
            return source
                .get(unit.code_range.as_range())
                .is_some_and(|text| return text.starts_with("match"));
        })
        .ok_or("expected control-flow unit in parsed fixture")?;
    assert!(!control.facts.first_body_reads.contains(&value));
    assert!(control.facts.whole_body_reads.contains(&value));
    return Ok(());
}
