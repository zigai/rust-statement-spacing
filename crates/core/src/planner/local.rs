//! Mandatory joins and group-independent spacing constraints.

use super::decisions::{Decision, blocked, enabled, join, optional_join, put, separate};
use super::guard_pair;
use super::visual;
use crate::config::{Bindings, Config, Expressions, GuardChain, ImmediateCheck, Separation, Tail};
use crate::model::{Form, ItemKind, Rule, RuleMask, UnitKind, UnitList};
use crate::relations::{Relationship, direct_producer, intersects, outputs, relationship};

pub(super) fn apply(
    config: &Config,
    source: &str,
    global_rules: RuleMask,
    list: &UnitList,
    cohesive: &[bool],
    decisions: &mut [Option<Decision>],
) {
    for i in 0..list.gaps.len() {
        let (Some(a), Some(b), Some(gap)) =
            (list.units.get(i), list.units.get(i + 1), list.gaps.get(i))
        else {
            continue;
        };
        if blocked(list, i) || !gap.joinable {
            continue;
        }
        let immediate = config.error_handling.immediate_result_option_check == ImmediateCheck::Join
            && a.kind == UnitKind::Let
            && b.facts.known
            && b.facts
                .check_of
                .as_ref()
                .is_some_and(|checked| return a.facts.definitions.contains(checked));
        if immediate && enabled(global_rules, b, Rule::ResultCheck) {
            put(
                global_rules,
                list,
                decisions,
                i,
                join(
                    Rule::ResultCheck,
                    "keep this Result/Option producer and its immediate check together",
                ),
            );
        }
    }

    // Rules that do not depend on group membership. Later group splitting cannot
    // remove a mandatory join or weaken one of these phase boundaries.
    for i in 0..list.gaps.len() {
        let (Some(a), Some(b)) = (list.units.get(i), list.units.get(i + 1)) else {
            continue;
        };
        if blocked(list, i) {
            continue;
        }
        let guards = guard_pair(config, a, b);
        if a.kind.is_item() || b.kind.is_item() {
            let kinds = [a.kind, b.kind];
            let function = kinds.contains(&UnitKind::Item(ItemKind::Function));
            let major = kinds.contains(&UnitKind::Item(ItemKind::Major));
            let required = (function && config.items.functions == Separation::Separate)
                || (major && config.items.major_items == Separation::Separate)
                || (kinds.iter().all(|kind| {
                    return matches!(
                        kind,
                        UnitKind::Item(
                            ItemKind::Import
                                | ItemKind::Constant
                                | ItemKind::Module
                                | ItemKind::Alias
                                | ItemKind::Compact
                        )
                    );
                }) && (a.kind != b.kind || !config.items.compact_declarations));
            if required {
                put(
                    global_rules,
                    list,
                    decisions,
                    i,
                    separate(
                        Rule::ItemSpacing,
                        60,
                        "separate these item/declaration groups",
                    ),
                );
            }
            continue;
        }
        if config.grouping.join_related
            && config.grouping.expressions == Expressions::Multiline
            && config.grouping.bindings == Bindings::Multiline
            && let Some(boundary) = visual::assertion_boundary(config, list, i)
        {
            if !boundary && list.gaps.get(i).is_none_or(|gap| return !gap.joinable) {
                continue;
            }
            let rule = if a.kind.is_binding() || b.kind.is_binding() {
                Rule::Bindings
            } else {
                Rule::Expressions
            };
            put(
                global_rules,
                list,
                decisions,
                i,
                if boundary {
                    separate(rule, 90, "separate this test assertion phase")
                } else {
                    join(rule, "keep this test assertion step together")
                },
            );
            continue;
        }
        let compact_policy = config.grouping.expressions != Expressions::Strict;
        let cleanup_exit = compact_policy
            && config.control_flow.compact_cleanup
            && (a.is_empty_loop
                || (a.kind.ordinary()
                    && (!a.facts.writes.is_empty() || !a.facts.mutating_receivers.is_empty())))
            && a.facts.known
            && b.is_bare_return
            && config.exits.tail != Tail::AlwaysSeparate;
        let attached_exit = compact_policy
            && config.exits.attached_loop_exit
            && b.is_loop_exit
            && a.facts.known
            && (!a.facts.writes.is_empty() || !a.facts.mutating_receivers.is_empty());
        let long = list.executable_count > config.exits.short_block_max_statements;
        // A completion acknowledgment has no local producer to look for. Empty
        // reads are meaningful only when extraction completed successfully.
        let completion =
            b.facts.known && b.facts.reads.is_empty() && !b.is_loop_exit && !b.is_bare_return;
        let completion_exit = compact_policy
            && completion
            && (b.kind == UnitKind::Exit || (b.is_tail && b.kind == UnitKind::Expression))
            && config.exits.tail == Tail::Smart;
        // Saving a value and then returning it is one phase. Resolved reads
        // remain affirmative evidence even when the destination is an alias
        // whose write target cannot be resolved.
        let cached_exit = compact_policy
            && config.grouping.shared_inputs
            && a.kind == UnitKind::Assignment
            && b.facts.known
            && intersects(&a.facts.reads, &b.facts.reads, config.grouping.self_fields);
        let guard_continuation = compact_policy
            && ((a.is_guard && config.control_flow.guard_chain == GuardChain::Allow)
                || (config.control_flow.guard_chain == GuardChain::Contextual
                    && visual::guard_continuation(a, b)));
        let guard_exit = guard_continuation
            && (b.kind == UnitKind::Exit || b.is_tail)
            && config.exits.tail == Tail::Smart;
        let data_continuation = compact_policy
            && config.control_flow.related_continuation
            && (b.kind.ordinary()
                || b.kind == UnitKind::Control
                || b.kind == UnitKind::Exit
                || b.shape.form == Form::Macro)
            && (direct_producer(&a.facts, &b.facts, &config.grouping)
                || (b.shape.form == Form::Macro
                    && intersects(
                        &outputs(&a.facts),
                        &b.facts.reads,
                        config.grouping.self_fields,
                    ))
                || ((b.facts.known || b.shape.form == Form::Macro)
                    && config.grouping.shared_inputs
                    && intersects(
                        &a.facts.header_reads,
                        &b.facts.reads,
                        config.grouping.self_fields,
                    )));
        let visual_tail = config.exits.tail == Tail::Visual && visual::compact_tail(list, a, b);
        let exit = b.kind == UnitKind::Exit
            && !cleanup_exit
            && !attached_exit
            && match config.exits.tail {
                Tail::AlwaysSeparate => true,
                Tail::Visual => !visual_tail,
                Tail::Preserve => false,
                Tail::Smart => {
                    long && !completion
                        && !cached_exit
                        && !guard_exit
                        && !direct_producer(&a.facts, &b.facts, &config.grouping)
                }
            };
        let tail = b.is_tail
            && !cleanup_exit
            && !attached_exit
            && match config.exits.tail {
                Tail::AlwaysSeparate => true,
                Tail::Visual => !visual_tail,
                Tail::Preserve => false,
                Tail::Smart => {
                    long && !completion
                        && !cached_exit
                        && !guard_exit
                        && cohesive.get(i) != Some(&true)
                        && !direct_producer(&a.facts, &b.facts, &config.grouping)
                }
            };
        if exit || tail {
            put(
                global_rules,
                list,
                decisions,
                i,
                separate(
                    Rule::Exit,
                    80,
                    "separate the block's final exit or value from the preceding phase",
                ),
            );
        }
        let receiver_continuation = compact_policy
            && config.control_flow.related_continuation
            && config.grouping.same_receiver
            && b.kind.ordinary()
            && !b.is_tail
            && b.facts.known
            && ((a.facts.known
                && intersects(
                    &a.facts.receivers,
                    &b.facts.receivers,
                    config.grouping.self_fields,
                ))
                // A resolved header input is affirmative evidence even if an
                // unrelated macro or deferred operation makes the body unknown.
                || (config.grouping.shared_inputs
                    && intersects(
                        &a.facts.header_reads,
                        &b.facts.receivers,
                        config.grouping.self_fields,
                    )));
        let drain_chain = compact_policy
            && config.control_flow.compact_cleanup
            && a.is_empty_loop
            && b.is_empty_loop
            && a.facts.known
            && b.facts.known;
        let unsafe_continuation = compact_policy
            && config.control_flow.related_continuation
            && a.kind == UnitKind::UnsafeBlock
            && (b.kind.is_binding()
                || b.kind == UnitKind::UnsafeBlock
                || relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Related);
        if a.kind.ends_block()
            && !(config.grouping.join_related && visual::guard_continuation(a, b))
            && !completion_exit
            && !visual_tail
            && !guards
            && !guard_continuation
            && !unsafe_continuation
            && !data_continuation
            && !receiver_continuation
            && !drain_chain
            && !cleanup_exit
            && cohesive.get(i) != Some(&true)
            && config.control_flow.after_block == Separation::Separate
        {
            put(
                global_rules,
                list,
                decisions,
                i,
                separate(
                    Rule::AfterBlock,
                    70,
                    "start a new logical group after this standalone block",
                ),
            );
        }
        // Normalization separates standalone conditionals, including setup
        // before a return. Only an early-exit guard can attach that return.
        if config.grouping.join_related
            && a.kind == UnitKind::Control
            && matches!(a.shape.form, Form::Conditional | Form::CallCheck)
            && !visual::guard_continuation(a, b)
            && config.control_flow.after_block == Separation::Separate
        {
            put(
                global_rules,
                list,
                decisions,
                i,
                separate(
                    Rule::AfterBlock,
                    95,
                    "start a new phase after this conditional",
                ),
            );
        }
        if b.kind == UnitKind::Control
            && a.kind.ordinary()
            && (if a.kind.is_binding() {
                config.grouping.bindings == Bindings::Multiline
            } else {
                config.grouping.expressions == Expressions::Multiline
            })
            && visual::boundary(config, source, list, i)
        {
            put(
                global_rules,
                list,
                decisions,
                i,
                separate(
                    Rule::ControlFlow,
                    30,
                    "separate this control flow from the preceding multiline phase",
                ),
            );
        }
        // Special constructs own their boundary, even when their specific rule
        // is disabled. A generic rule must not reproduce a suppressed rule.
        if b.kind == UnitKind::Control || b.kind == UnitKind::Exit || b.is_tail {
            continue;
        }
        if config.grouping.bindings == Bindings::Multiline
            && a.kind.is_binding()
            && b.shape.form == Form::Macro
            && !b.facts.reads.is_empty()
            && !intersects(
                &a.facts.definitions,
                &b.facts.reads,
                config.grouping.self_fields,
            )
        {
            put(
                global_rules,
                list,
                decisions,
                i,
                separate(
                    Rule::Bindings,
                    30,
                    "separate setup from this macro operation",
                ),
            );
        }
        if !a.kind.ordinary() || !b.kind.ordinary() {
            continue;
        }
        if cohesive.get(i) == Some(&true) {
            continue;
        }
        if a.kind.is_binding() && b.kind.is_binding() {
            let required = match config.grouping.bindings {
                Bindings::Consecutive | Bindings::Preserve => false,
                Bindings::SameKind => a.kind != b.kind,
                Bindings::Multiline => visual::boundary(config, source, list, i),
                Bindings::Related => {
                    relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Unrelated
                }
            };
            if required {
                put(
                    global_rules,
                    list,
                    decisions,
                    i,
                    separate(Rule::Bindings, 30, "separate these binding phases"),
                );
            }
        } else {
            let required = match config.grouping.expressions {
                Expressions::Preserve => false,
                Expressions::Strict => true,
                Expressions::Multiline => visual::boundary(config, source, list, i),
                Expressions::Related => {
                    relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Unrelated
                }
            };
            if required {
                let rule = if a.kind.is_binding() || b.kind.is_binding() {
                    Rule::Bindings
                } else {
                    Rule::Expressions
                };
                put(
                    global_rules,
                    list,
                    decisions,
                    i,
                    separate(rule, 30, "separate these operation phases"),
                );
            }
        }
    }
}

pub(super) fn normalize(
    config: &Config,
    source: &str,
    global_rules: RuleMask,
    list: &UnitList,
    decisions: &mut [Option<Decision>],
) {
    if !config.grouping.join_related {
        return;
    }
    for i in 0..list.gaps.len() {
        let (Some(a), Some(b), Some(gap)) =
            (list.units.get(i), list.units.get(i + 1), list.gaps.get(i))
        else {
            continue;
        };
        if blocked(list, i) || !gap.joinable {
            continue;
        }
        let rule = if a.kind.is_item() || b.kind.is_item() {
            if a.kind != b.kind
                || !config.items.compact_declarations
                || !matches!(
                    a.kind,
                    UnitKind::Item(
                        ItemKind::Import
                            | ItemKind::Constant
                            | ItemKind::Module
                            | ItemKind::Alias
                            | ItemKind::Compact
                    )
                )
            {
                continue;
            }
            Rule::ItemSpacing
        } else if b.kind == UnitKind::Exit || (b.is_tail && b.kind != UnitKind::Control) {
            if config.exits.tail == Tail::Preserve || !visual::guard_continuation(a, b) {
                continue;
            }
            Rule::Exit
        } else if b.kind == UnitKind::Control {
            // Result checks own their boundary even with their rule suppressed
            // or configured to preserve. Do not replace them with a generic join.
            if b.facts.check_of.is_some()
                || config.grouping.bindings == Bindings::Preserve
                || config.grouping.expressions == Expressions::Preserve
                || config.grouping.expressions == Expressions::Strict
                || !a.kind.ordinary()
                || !b.is_guard
                || b.shape.form == Form::CallCheck
                || !(intersects(
                    &a.facts.definitions,
                    &b.facts.header_reads,
                    config.grouping.self_fields,
                ) || (a.kind == UnitKind::Expression
                    && config.control_flow.guard_chain == GuardChain::Contextual
                    && !visual::validation_stages(list)
                    && intersects(
                        &a.facts.reads,
                        &b.facts.header_reads,
                        config.grouping.self_fields,
                    )))
            {
                continue;
            }
            Rule::ControlFlow
        } else {
            if !a.kind.ordinary() || !b.kind.ordinary() {
                continue;
            }
            let bindings = a.kind.is_binding() && b.kind.is_binding();
            if (a.kind.is_binding() || b.kind.is_binding())
                && config.grouping.bindings == Bindings::Preserve
                || (!bindings
                    && matches!(
                        config.grouping.expressions,
                        Expressions::Preserve | Expressions::Strict
                    ))
            {
                continue;
            }
            let short_setup = a.kind == UnitKind::Let
                && b.kind == UnitKind::Let
                && matches!(
                    config.grouping.bindings,
                    Bindings::Consecutive | Bindings::SameKind | Bindings::Multiline
                )
                && [a, b].iter().all(|unit| {
                    return !matches!(
                        unit.shape.form,
                        Form::LetElse
                            | Form::Branch
                            | Form::Conditional
                            | Form::Macro
                            | Form::Closure
                    ) && source
                        .get(unit.code_range.as_range())
                        .is_some_and(|text| return !text.contains('\n'));
                });
            if !short_setup && !visual::related_join(config, source, a, b) {
                continue;
            }
            if a.kind.is_binding() || b.kind.is_binding() {
                Rule::Bindings
            } else {
                Rule::Expressions
            }
        };
        put(global_rules, list, decisions, i, optional_join(rule));
    }
}

pub(super) fn cap_blank_lines(
    global_rules: RuleMask,
    list: &UnitList,
    decisions: &mut [Option<Decision>],
) {
    for i in 0..list.gaps.len() {
        if list.gaps.get(i).is_some_and(|g| return g.blank_lines > 1) {
            put(
                global_rules,
                list,
                decisions,
                i,
                separate(
                    Rule::Layout,
                    1,
                    "use at most one blank line at this boundary",
                ),
            );
        }
    }
}
