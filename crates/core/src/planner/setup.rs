//! Monotone setup-group splitting over the effective boundary layout.

use super::decisions::{
    Decision, blocked, effective_blank, enabled, mandatory_join, optional_join, put, separate,
};
use super::guard_pair;
use super::visual;
use crate::config::{Bindings, Config, Expressions, GuardChain, Overflow, UseIn};
use crate::model::{Form, Rule, RuleMask, UnitKind, UnitList};
use crate::relations::{control_inputs, intersects, outputs};

pub(super) fn apply(
    config: &Config,
    global_rules: RuleMask,
    list: &UnitList,
    cohesive: &[bool],
    decisions: &mut [Option<Decision>],
) {
    // Monotone closure: optional joins can strengthen into required separators.
    // Reconsider affected groups before exposing edits, not on a later fix pass.
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
                || effective_blank(list, decisions, gap, config.grouping.join_related) > 0
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
            if config.control_flow.guard_chain == GuardChain::Contextual
                && config.grouping.expressions != Expressions::Strict
                && config.grouping.max_before_control > 0
                && unit.is_guard
                && gap_unit.kind == UnitKind::Expression
                && intersects(
                    &gap_unit.facts.reads,
                    &unit.facts.header_reads,
                    config.grouping.self_fields,
                )
                && unit.shape.form != Form::CallCheck
            {
                if visual::validation_stages(list) {
                    changed |= put(
                        global_rules,
                        list,
                        decisions,
                        gap,
                        separate(Rule::ControlFlow, 90, "separate these validation stages"),
                    );
                }
                continue;
            }
            if config.grouping.bindings == Bindings::Multiline && unit.shape.form == Form::Loop {
                let mut start = gap;
                while start > 0
                    && !blocked(list, start - 1)
                    && effective_blank(list, decisions, start - 1, config.grouping.join_related)
                        == 0
                    && list
                        .units
                        .get(start - 1)
                        .is_some_and(|unit| return unit.kind.ordinary())
                {
                    start -= 1;
                }
                if control - start > config.grouping.max_before_control
                    && list
                        .units
                        .iter()
                        .take(control)
                        .skip(start)
                        .all(|unit| return unit.kind.is_binding())
                {
                    let scalar_count = list
                        .units
                        .iter()
                        .take(control)
                        .skip(start)
                        .filter(|unit| {
                            return unit.kind == UnitKind::Let
                                && unit.shape.mutable
                                && unit.shape.form == Form::Literal;
                        })
                        .count();
                    let boundary = if config.grouping.max_before_control > 0
                        && gap_unit.shape.form == Form::Literal
                        && gap_unit.shape.mutable
                        && scalar_count == 1
                    {
                        gap.saturating_sub(1)
                    } else {
                        gap
                    };
                    changed |= put(
                        global_rules,
                        list,
                        decisions,
                        boundary,
                        separate(
                            Rule::ControlFlow,
                            90,
                            "keep this loop's complete setup phase intact",
                        ),
                    );
                    continue;
                }
            }
            if !unit.facts.known
                && unit.facts.writes.is_empty()
                && unit.facts.mutating_receivers.is_empty()
                && config.grouping.max_before_control != 0
            {
                continue;
            }
            let mut group_start = gap;
            while group_start > 0 {
                let previous_gap = group_start - 1;
                if blocked(list, previous_gap)
                    || effective_blank(list, decisions, previous_gap, config.grouping.join_related)
                        > 0
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
                let initializes_updated_state = config.grouping.expressions != Expressions::Strict
                    && (unit.facts.known || unit.shape.form == Form::Loop)
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
                // Unknown effects cannot prove a read relationship, but do not
                // invalidate an independently resolved non-deferred mutation.
                if !unit.facts.known && !initializes_updated_state {
                    unknown = true;
                    break;
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
                    .is_some_and(mandatory_join)
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
                        let accumulators = control - accumulator_start;
                        let retained = if config.grouping.bindings == Bindings::Multiline
                            && accumulators > limit
                        {
                            mandatory
                        } else {
                            accumulators.min(limit).max(mandatory)
                        };
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
            if config.grouping.join_related
                && config.grouping.bindings != Bindings::Preserve
                && config.grouping.expressions != Expressions::Preserve
                && unit.shape.form == Form::Loop
                && accumulator_start < control
                && limit > 0
                && boundary != Some(gap)
                && list.gaps.get(gap).is_some_and(|gap| return gap.joinable)
            {
                changed |= put(
                    global_rules,
                    list,
                    decisions,
                    gap,
                    optional_join(Rule::ControlFlow),
                );
            }
            if let Some(mut boundary) = boundary {
                // Expand the retained suffix backwards to the start of a
                // joined component; never split a mandatory pair in its middle.
                while decisions
                    .get(boundary)
                    .and_then(|d| return d.as_ref())
                    .is_some_and(mandatory_join)
                    && boundary > group_start
                {
                    boundary -= 1;
                }
                if decisions
                    .get(boundary)
                    .and_then(|d| return d.as_ref())
                    .is_some_and(mandatory_join)
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
