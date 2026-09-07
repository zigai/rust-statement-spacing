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

fn object_accesses<'facts>(
    facts: &'facts Facts,
    settings: &'facts Grouping,
) -> impl Iterator<Item = &'facts Place> {
    return facts
        .writes
        .iter()
        .chain(
            facts
                .reads
                .iter()
                .filter(move |_| return settings.shared_inputs),
        )
        .chain(
            facts
                .receivers
                .iter()
                .filter(move |_| return settings.same_receiver),
        );
}

fn same_object(a: &Facts, b: &Facts, settings: &Grouping) -> bool {
    return object_accesses(a, settings).any(|a| {
        return object_accesses(b, settings).any(|b| {
            return a.local == b.local
                && (!(a.is_self || b.is_self) || places_overlap(a, b, settings.self_fields));
        });
    });
}

fn mutates(facts: &Facts) -> bool {
    return !facts.writes.is_empty() || !facts.mutating_receivers.is_empty();
}

pub(crate) fn relationship(a: &Facts, b: &Facts, settings: &Grouping) -> Relationship {
    if !a.known || !b.known {
        return Relationship::Unknown;
    }
    if direct_producer(a, b, settings)
        || intersects(&outputs(b), &a.reads, settings.self_fields)
        || (settings.same_receiver && intersects(&a.receivers, &b.receivers, settings.self_fields))
        || (settings.shared_inputs && intersects(&a.reads, &b.reads, settings.self_fields))
        || (settings.same_object && same_object(a, b, settings))
        || (settings.same_callee && !a.direct_callees.is_disjoint(&b.direct_callees))
        || (settings.consecutive_mutations && mutates(a) && mutates(b))
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
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Related);
        c.grouping.self_fields = SelfFields::Distinct;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
    }

    #[test]
    fn producer_policy_does_not_conflate_unrelated_places() {
        let c = Config::default();
        for (left, right) in [
            (
                Place {
                    local: "self-id".into(),
                    projections: vec!["arch".into()],
                    is_self: true,
                },
                Place {
                    local: "other-self-id".into(),
                    projections: vec!["machine".into()],
                    is_self: true,
                },
            ),
            (
                Place {
                    local: "config-id".into(),
                    projections: vec!["arch".into()],
                    is_self: false,
                },
                Place {
                    local: "config-id".into(),
                    projections: vec!["machine".into()],
                    is_self: false,
                },
            ),
        ] {
            let mut a = facts(&[], &[]);
            let mut b = facts(&[], &[]);
            a.writes.insert(left);
            b.reads.insert(right);
            assert!(!direct_producer(&a, &b, &c.grouping));
        }
    }

    #[test]
    fn same_callee_uses_identity_and_can_be_disabled() {
        let mut a = facts(&[], &["input-a"]);
        let mut b = facts(&[], &["input-b"]);
        a.direct_callees.insert("crate-a:function".into());
        b.direct_callees.insert("crate-b:function".into());
        let mut c = Config::default();
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
        b.direct_callees = a.direct_callees.clone();
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Related);
        c.grouping.same_callee = false;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
        c.grouping.same_callee = true;
        b.known = false;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unknown);
    }

    #[test]
    fn aggregate_grouping_preserves_precise_producers_and_opt_outs() {
        let mut a = facts(&[], &[]);
        let mut b = facts(&[], &[]);
        a.writes.insert(Place {
            local: "inputs".into(),
            projections: vec!["enabled".into()],
            is_self: false,
        });
        b.receivers.insert(Place {
            local: "inputs".into(),
            projections: vec!["keys".into()],
            is_self: false,
        });
        let mut c = Config::default();
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Related);
        assert!(!direct_producer(&a, &b, &c.grouping));
        c.grouping.same_object = false;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
        c.grouping.same_object = true;
        c.grouping.same_receiver = false;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
        c.grouping.same_receiver = true;
        b.receivers.clear();
        b.receivers.insert(Place::local("other-inputs"));
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
    }

    #[test]
    fn shared_input_opt_out_also_disables_aggregate_read_grouping() {
        let mut a = facts(&[], &[]);
        let mut b = facts(&[], &[]);
        for (facts, field) in [(&mut a, "width"), (&mut b, "height")] {
            facts.reads.insert(Place {
                local: "dimensions".into(),
                projections: vec![field.into()],
                is_self: false,
            });
        }
        let mut c = Config::default();
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Related);
        c.grouping.shared_inputs = false;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
    }

    #[test]
    fn mutation_grouping_requires_two_mutations_not_a_pure_call() {
        let mut a = facts(&[], &["collection"]);
        a.receivers.insert(Place::local("collection"));
        let mut b = facts(&[], &[]);
        b.writes.insert(Place::local("counter"));
        let mut c = Config::default();
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
        a.mutating_receivers.insert(Place::local("collection"));
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Related);
        assert!(!direct_producer(&a, &b, &c.grouping));
        c.grouping.consecutive_mutations = false;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Unrelated);
        a.mutating_receivers.clear();
        a.writes.insert(Place::local("collection"));
        c.grouping.consecutive_mutations = true;
        assert_eq!(relationship(&a, &b, &c.grouping), Relationship::Related);
    }
}
