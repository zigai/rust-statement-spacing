//! Relationships between resolved local places.

use std::collections::BTreeSet;

use crate::config::{Grouping, SelfFields, UseIn};
use crate::model::{Facts, Place};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Whether two units have a known semantic connection.
pub(crate) enum Relationship {
    /// A producer, receiver, or shared-input connection exists.
    Related,
    /// Facts are known and no configured connection exists.
    Unrelated,
    /// At least one unit lacks sufficient compiler facts.
    Unknown,
}

fn places_overlap(a: &Place, b: &Place, mode: SelfFields) -> bool {
    if a.local != b.local {
        return false;
    }
    if mode == SelfFields::Root && a.is_self && b.is_self {
        return true;
    }
    // Writing/reading an aggregate can relate to one of its fields. Distinct
    // siblings do not relate merely because they have the same aggregate root.
    return a.projections.starts_with(&b.projections) || b.projections.starts_with(&a.projections);
}

pub(crate) fn intersects(a: &BTreeSet<Place>, b: &BTreeSet<Place>, mode: SelfFields) -> bool {
    return a.iter().any(|left| {
        return b
            .iter()
            .any(|right| return places_overlap(left, right, mode));
    });
}

pub(crate) fn outputs(facts: &Facts) -> BTreeSet<Place> {
    return facts.definitions.union(&facts.writes).cloned().collect();
}

pub(crate) fn direct_producer(a: &Facts, b: &Facts, settings: &Grouping) -> bool {
    return a.known && b.known && intersects(&outputs(a), &b.reads, settings.self_fields);
}

pub(crate) fn relationship(a: &Facts, b: &Facts, settings: &Grouping) -> Relationship {
    if !a.known || !b.known {
        return Relationship::Unknown;
    }
    if direct_producer(a, b, settings)
        || intersects(&outputs(b), &a.reads, settings.self_fields)
        || (settings.same_receiver && intersects(&a.receivers, &b.receivers, settings.self_fields))
        || (settings.shared_inputs && intersects(&a.reads, &b.reads, settings.self_fields))
    {
        return Relationship::Related;
    } else {
        return Relationship::Unrelated;
    }
}

pub(crate) fn control_inputs(facts: &Facts, mode: UseIn) -> BTreeSet<Place> {
    let mut inputs = facts.header_reads.clone();
    match mode {
        UseIn::Header => {}
        UseIn::HeaderOrFirstBodyStatement => inputs.extend(facts.first_body_reads.clone()),
        UseIn::WholeBody => inputs.extend(facts.whole_body_reads.clone()),
    }
    return inputs;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn facts(defs: &[&str], reads: &[&str]) -> Facts {
        return Facts {
            known: true,
            definitions: defs.iter().map(|name| return Place::local(*name)).collect(),
            reads: reads
                .iter()
                .map(|name| return Place::local(*name))
                .collect(),
            ..Facts::default()
        };
    }

    #[test]
    fn same_spelling_with_distinct_binding_id_is_unrelated() {
        let c = Config::default();
        assert_eq!(
            relationship(
                &facts(&["scope1:x"], &[]),
                &facts(&[], &["scope2:x"]),
                &c.grouping
            ),
            Relationship::Unrelated
        );
    }

    #[test]
    fn self_field_projection_policy() {
        let mut a = facts(&[], &[]);
        let mut b = facts(&[], &[]);
        a.receivers.insert(Place {
            local: "self-id".into(),
            projections: vec!["cache".into()],
            is_self: true,
        });
        b.receivers.insert(Place {
            local: "self-id".into(),
            projections: vec!["socket".into()],
            is_self: true,
        });
        let mut c = Config::default();
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
        c.grouping.self_fields = SelfFields::Root;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Related);
    }
}
