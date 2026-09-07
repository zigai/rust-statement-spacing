//! Monotone setup-group splitting over the effective boundary layout.

use super::decisions::{Decision, blocked, effective_blank, enabled, put, separate};
use super::guard_pair;
use crate::config::{Config, Expressions, Overflow, UseIn};
use crate::model::{Rule, RuleMask, UnitKind, UnitList};
use crate::relations::{control_inputs, intersects, outputs};

pub(super) fn apply(
    config: &Config,
    global_rules: RuleMask,
    list: &UnitList,
    cohesive: &[bool],
    decisions: &mut [Option<Decision>],
) {
    // Monotone closure: setup splitting adds separators only. Reconsider affected
    // groups before exposing edits, rather than requiring another user fix pass.
    for _ in 0..=list.gaps.len() {
        let mut changed = false;
        for control in 1..list.units.len() {
            let Some(unit) = list.units.get(control) else {
                continue;
            };
            let gap = control - 1;
            let Some(gap_unit) = list.units.get(gap) else {
                continue;
            };
            if unit.kind != UnitKind::Control
                || !gap_unit.kind.ordinary()
                || !enabled(global_rules, unit, Rule::ControlFlow)
                || blocked(list, gap)
                || guard_pair(config, gap_unit, unit)
                || effective_blank(list, decisions, gap) > 0
                || cohesive.get(gap) == Some(&true)
            {
                continue;
            }
            if config.control_flow.compact_cleanup
                && config.grouping.expressions != Expressions::Strict
                && unit.is_empty_loop
                && gap_unit.facts.known
                && (!gap_unit.facts.writes.is_empty()
                    || !gap_unit.facts.mutating_receivers.is_empty())
            {
                continue;
            }
            if !unit.facts.known && config.grouping.max_before_control != 0 {
                continue;
            }
            let mut group_start = gap;
            while group_start > 0 {
                let previous_gap = group_start - 1;
                if blocked(list, previous_gap)
                    || effective_blank(list, decisions, previous_gap) > 0
                    || !list
                        .units
                        .get(previous_gap)
                        .is_some_and(|u| return u.kind.ordinary())
                {
                    break;
                }
                group_start -= 1;
            }
            let mut needed = control_inputs(&unit.facts, config.grouping.use_in);
            let mut accepted_start = control;
            let mut accumulator_start = control;
            let mut unknown = false;
            for candidate in (group_start..control).rev() {
                let Some(setup) = list.units.get(candidate) else {
                    break;
                };
                if !setup.kind.ordinary() {
                    break;
                }
                if !setup.facts.known {
                    unknown = true;
                    break;
                }
                let mut provided = outputs(&setup.facts);
                // Initializing state that the control body updates is setup even
                // when the update is nested or follows another statement. Only
                // affirmative non-deferred mutation facts extend the read scope;
                // a later read or shared receiver alone is not enough. Keep this
                // in the bounded setup scan, not an indivisible producer pair.
                let initializes_updated_state = unit.facts.known
                    && config.grouping.expressions != Expressions::Strict
                    && config.grouping.use_in != UseIn::Header
                    && (intersects(&provided, &unit.facts.writes, config.grouping.self_fields)
                        || intersects(
                            &provided,
                            &unit.facts.mutating_receivers,
                            config.grouping.self_fields,
                        ));
                // A bounded suffix of fresh accumulators belongs to the loop
                // that fills it, even when earlier bindings form a larger group.
                // Mutation operations alone are not fresh initializations.
                if initializes_updated_state
                    && candidate + 1 == accumulator_start
                    && (intersects(
                        &setup.facts.definitions,
                        &unit.facts.writes,
                        config.grouping.self_fields,
                    ) || intersects(
                        &setup.facts.definitions,
                        &unit.facts.mutating_receivers,
                        config.grouping.self_fields,
                    ))
                {
                    accumulator_start = candidate;
                }
                if config.grouping.same_receiver {
                    // Receiver-centred setup is a grouping heuristic, not a
                    // claim that the called method mutates its receiver.
                    provided.extend(setup.facts.receivers.clone());
                }
                if !initializes_updated_state
                    && !intersects(&provided, &needed, config.grouping.self_fields)
                {
                    break;
                }
                needed.extend(setup.facts.reads.clone());
                accepted_start = candidate;
            }
            if unknown && accepted_start == control && config.grouping.max_before_control != 0 {
                continue;
            }
            // A mandatory producer/check pair is indivisible, not an exemption
            // for every unrelated statement that happens to precede it.
            let mut atomic_start = control;
            while atomic_start > group_start
                && decisions
                    .get(atomic_start - 1)
                    .and_then(|d| return d.as_ref())
                    .is_some_and(|d| return d.blanks == 0)
            {
                atomic_start -= 1;
            }
            let mandatory = control - atomic_start;
            let total = control - group_start;
            let related = control - accepted_start;
            let limit = config.grouping.max_before_control;
            let boundary = match config.grouping.overflow {
                Overflow::WholeGroup => {
                    if related != total || total > limit {
                        let retained = (control - accumulator_start).min(limit).max(mandatory);
                        if retained == 0 {
                            Some(gap)
                        } else {
                            let start = control - retained;
                            (start > group_start).then_some(start.saturating_sub(1))
                        }
                    } else {
                        None
                    }
                }
                Overflow::RelatedSuffix => {
                    let accepted = related.min(limit).max(mandatory);
                    if accepted == 0 {
                        Some(gap)
                    } else {
                        let start = control - accepted;
                        (start > group_start).then(|| return start - 1)
                    }
                }
            };
            if let Some(mut boundary) = boundary {
                // Expand the retained suffix backwards to the start of a
                // joined component; never split a mandatory pair in its middle.
                while decisions
                    .get(boundary)
                    .and_then(|d| return d.as_ref())
                    .is_some_and(|d| return d.blanks == 0)
                    && boundary > group_start
                {
                    boundary -= 1;
                }
                if decisions
                    .get(boundary)
                    .and_then(|d| return d.as_ref())
                    .is_some_and(|d| return d.blanks == 0)
                {
                    continue;
                }
                let mut decision = separate(
                    Rule::ControlFlow,
                    90,
                    "separate excess or unrelated setup from this control-flow group",
                );
                decision.anchor = Some(unit.anchor);
                changed |= put(global_rules, list, decisions, boundary, decision);
            }
        }
        if !changed {
            break;
        }
    }
}
