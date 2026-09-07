use std::collections::BTreeSet;

use rust_statement_spacing_core::{ByteRange, Facts, Place, RuleMask};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Compiler-resolved operation contributing to a unit relationship.
pub enum EventKind {
    /// Read of a local place.
    Read,
    /// Introduction of a binding.
    Define,
    /// Assignment or exposure of a place through a mutable reference/raw address.
    Write,
    /// Use of a place as a method receiver.
    Receiver,
    /// Receiver compiler-adjusted to a mutable reference.
    MutatingReceiver,
    /// Resolved identity of a direct source expression-statement call.
    DirectCallee(String),
    /// Compiler-confirmed Result/Option inspection.
    Inspect,
    /// Operation whose effects could not be resolved.
    Unknown,
}

#[derive(Clone, Debug)]
/// One compiler event located in normalized original source.
pub struct Event {
    /// Event span in normalized source bytes.
    pub range: ByteRange,
    /// Operation performed at this span.
    pub kind: EventKind,
    /// Resolved local place, when the operation identifies one.
    pub place: Option<Place>,
}

#[derive(Clone, Debug)]
/// Active compiler node corresponding to a direct source unit.
pub struct Anchor {
    /// Compiler-node span in normalized source bytes.
    pub range: ByteRange,
    /// Opaque compiler identity used to scope diagnostics.
    pub id: usize,
    /// Spacing lints enabled at this compiler node.
    pub enabled: RuleMask,
}

#[derive(Clone, Debug, Default)]
/// Compiler anchors and local-place events for one source file.
pub struct SemanticIndex {
    /// Active original-source nodes eligible for linting.
    pub anchors: Vec<Anchor>,
    /// Resolved reads, definitions, writes, receivers, and checks.
    pub events: Vec<Event>,
}

impl SemanticIndex {
    /// Finds an exact-start anchor, permitting an optional trailing semicolon.
    pub fn anchor(&self, range: ByteRange) -> Option<&Anchor> {
        return self
            .anchors
            .iter()
            .filter(|anchor| {
                return anchor.range.start == range.start
                    && anchor.range.end <= range.end
                    && range.end - anchor.range.end <= 1;
            })
            .max_by_key(|anchor| return anchor.range.end);
    }

    /// Aggregates events within a unit, excluding deferred bodies.
    ///
    /// `declaration` limits binding definitions; `header` and `first_body` select
    /// control-flow inputs. Opaque or unknown operations make the facts unknown.
    pub fn facts(
        &self,
        range: ByteRange,
        declaration: Option<ByteRange>,
        header: Option<ByteRange>,
        first_body: &[ByteRange],
        deferred: &[ByteRange],
        opaque: bool,
    ) -> Facts {
        let events: Vec<_> = self
            .events
            .iter()
            .filter(|event| {
                return range.contains(event.range)
                    && !deferred
                        .iter()
                        .any(|body| return body.contains(event.range));
            })
            .collect();
        let mut facts = Facts {
            known: !opaque
                && !events
                    .iter()
                    .any(|event| return event.kind == EventKind::Unknown),
            ..Facts::default()
        };
        for event in &events {
            if let EventKind::DirectCallee(callee) = &event.kind {
                if event.range.start == range.start && range.end - event.range.end <= 1 {
                    facts.direct_callees.insert(callee.clone());
                }
                continue;
            }
            let Some(place) = &event.place else {
                continue;
            };
            match &event.kind {
                EventKind::Define => {
                    if declaration.is_some_and(|pat| return pat.contains(event.range)) {
                        facts.definitions.insert(place.clone());
                    }
                }
                EventKind::Read => {
                    // Do not add `self` as an incidental aggregate read when the
                    // original written access was actually `self.cache`.
                    let incidental = events.iter().any(|outer| {
                        return outer.kind == EventKind::Read
                            && outer.range != event.range
                            && outer.range.contains(event.range)
                            && outer.place.as_ref().is_some_and(|p| {
                                return p.local == place.local
                                    && p.projections.len() > place.projections.len();
                            });
                    });
                    if !incidental {
                        facts.reads.insert(place.clone());
                        facts.whole_body_reads.insert(place.clone());
                        if header.is_some_and(|h| return h.contains(event.range)) {
                            facts.header_reads.insert(place.clone());
                        }
                        if first_body.iter().any(|b| return b.contains(event.range)) {
                            facts.first_body_reads.insert(place.clone());
                        }
                    }
                }
                EventKind::Write => {
                    facts.writes.insert(place.clone());
                }
                EventKind::Receiver => {
                    facts.receivers.insert(place.clone());
                }
                EventKind::MutatingReceiver => {
                    facts.mutating_receivers.insert(place.clone());
                }
                EventKind::Inspect => {
                    if header.is_some_and(|h| return h.contains(event.range)) {
                        // Multiple inspected values are deliberately not a narrow pair.
                        if facts.check_of.as_ref().is_some_and(|p| return p != place) {
                            facts.check_of = None;
                            facts.known = false;
                        } else {
                            facts.check_of = Some(place.clone());
                        }
                    }
                }
                EventKind::Unknown | EventKind::DirectCallee(_) => {}
            }
        }
        return facts;
    }

    /// Creates synthetic anchors for structural tests, without resolved events.
    /// Production callers must supply compiler anchors instead.
    pub fn structural_anchors(ranges: impl IntoIterator<Item = ByteRange>) -> Self {
        return Self {
            anchors: ranges
                .into_iter()
                .enumerate()
                .map(|(id, range)| {
                    return Anchor {
                        range,
                        id,
                        enabled: RuleMask::all(),
                    };
                })
                .collect(),
            events: Vec::new(),
        };
    }

    /// Collects resolved places read inside the supplied source range.
    pub fn local_reads(&self, range: ByteRange) -> BTreeSet<Place> {
        return self
            .events
            .iter()
            .filter(|event| return event.kind == EventKind::Read && range.contains(event.range))
            .filter_map(|event| return event.place.clone())
            .collect();
    }
}
