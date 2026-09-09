//! Compact receiver operations connected through intervening local setup.

use super::decisions::blocked;
use crate::config::{Config, Expressions, Tail};
use crate::model::{UnitKind, UnitList};
use crate::relations::intersects;
use std::collections::BTreeSet;

pub(super) fn cohesive(config: &Config, list: &UnitList) -> Vec<bool> {
    let mut compact = Vec::new();
    if config.grouping.expressions != Expressions::Related
        || !config.grouping.same_receiver
        || !config.control_flow.related_continuation
        || config.grouping.max_before_control == 0
    {
        return compact;
    }

    let mut start = 0;
    while let Some(anchor) = list.units.get(start) {
        if anchor.kind != UnitKind::Expression || anchor.facts.receivers.is_empty() {
            start += 1;
            continue;
        }

        let mut definitions = BTreeSet::new();
        let mut pending = BTreeSet::new();
        let mut has_setup = false;
        let mut has_control = false;
        let mut end = start;

        for (index, unit) in list.units.iter().enumerate().skip(start + 1) {
            let gap = index - 1;
            if blocked(list, gap)
                || list.gaps.get(gap).is_none_or(|gap| {
                    return (!config.grouping.join_related && gap.blank_lines != 0)
                        || !gap.joinable;
                })
                || !(unit.kind.ordinary() || unit.kind == UnitKind::Control)
                || (unit.is_tail
                    && (unit.kind != UnitKind::Control
                        || config.exits.tail == Tail::AlwaysSeparate))
            {
                break;
            }

            let receiver = intersects(
                &anchor.facts.receivers,
                &unit.facts.receivers,
                config.grouping.self_fields,
            ) || intersects(
                &anchor.facts.receivers,
                &unit.facts.header_reads,
                config.grouping.self_fields,
            );

            let uses_local = unit
                .facts
                .reads
                .iter()
                .any(|read| return definitions.contains(read.local.as_str()));

            let uses_receiver = receiver
                || intersects(
                    &anchor.facts.receivers,
                    &unit.facts.reads,
                    config.grouping.self_fields,
                );

            if unit.kind == UnitKind::Control {
                if has_control || !(uses_receiver || uses_local) {
                    break;
                }

                has_control = true;
            } else if unit.kind != UnitKind::Let && !(uses_receiver || uses_local) {
                break;
            }

            // Only affirmative source dependencies are used. Unknown/deferred
            // effects do not establish a connection, but do not erase a known
            // argument read either. All introduced locals must be consumed.
            for read in &unit.facts.reads {
                pending.remove(read.local.as_str());
            }

            if unit.kind == UnitKind::Let {
                if has_control || unit.facts.definitions.is_empty() {
                    break;
                }

                has_setup = true;

                for place in &unit.facts.definitions {
                    definitions.insert(place.local.as_str());
                    pending.insert(place.local.as_str());
                }
            }

            if has_setup && pending.is_empty() && receiver {
                end = index;
            }
        }

        if end > start {
            if compact.is_empty() {
                compact.resize(list.gaps.len(), false);
            }

            if let Some(gaps) = compact.get_mut(start..end) {
                gaps.fill(true);
            }

            start = end;
        } else {
            start += 1;
        }
    }

    return compact;
}
