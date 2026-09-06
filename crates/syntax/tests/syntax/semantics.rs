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
    let place = Place::local("captured");
    let index = SemanticIndex {
        anchors: vec![],
        events: vec![Event {
            range: ByteRange::new(10, 18),
            kind: EventKind::Read,
            place: Some(place),
        }],
    };
    let facts = index.facts(
        ByteRange::new(0, 30),
        None,
        None,
        &[],
        &[ByteRange::new(8, 25)],
        true,
    );
    assert!(!facts.known);
    assert!(facts.reads.is_empty());
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
