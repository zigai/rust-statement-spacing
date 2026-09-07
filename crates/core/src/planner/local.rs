//! Mandatory joins and group-independent spacing constraints.

use super::decisions::{Decision, blocked, enabled, join, put, separate};
use super::guard_pair;
use crate::config::{Bindings, Config, Expressions, GuardChain, ImmediateCheck, Separation, Tail};
use crate::model::{ItemKind, Rule, RuleMask, UnitKind, UnitList};
use crate::relations::{Relationship, direct_producer, intersects, relationship};

pub(super) fn apply(
    config: &Config,
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
            && a.facts.known
            && b.facts.known
            && a.facts.definitions.len() == 1
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
        } else if config.grouping.join_related
            && a.kind.is_binding()
            && b.kind.ordinary()
            && !b.is_tail
            && direct_producer(&a.facts, &b.facts, &config.grouping)
        {
            put(
                global_rules,
                list,
                decisions,
                i,
                join(
                    Rule::Bindings,
                    "keep this direct producer/consumer pair together",
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
                || (kinds
                    .iter()
                    .all(|kind| return *kind == UnitKind::Item(ItemKind::Compact))
                    && !config.items.compact_declarations);
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
        let guard_continuation =
            compact_policy && a.is_guard && config.control_flow.guard_chain == GuardChain::Allow;
        let guard_exit = guard_continuation
            && (b.kind == UnitKind::Exit || b.is_tail)
            && config.exits.tail == Tail::Smart;
        let data_continuation = compact_policy
            && config.control_flow.related_continuation
            && (b.kind.ordinary() || b.kind == UnitKind::Control || b.kind == UnitKind::Exit)
            && (direct_producer(&a.facts, &b.facts, &config.grouping)
                || (b.facts.known
                    && config.grouping.shared_inputs
                    && intersects(
                        &a.facts.header_reads,
                        &b.facts.reads,
                        config.grouping.self_fields,
                    )));
        let exit = b.kind == UnitKind::Exit
            && !cleanup_exit
            && !attached_exit
            && match config.exits.tail {
                Tail::AlwaysSeparate => true,
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
            && !completion_exit
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
        // Special constructs own their boundary, even when their specific rule
        // is disabled. A generic rule must not reproduce a suppressed rule.
        if b.kind == UnitKind::Control || b.kind == UnitKind::Exit || b.is_tail {
            continue;
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
                    separate(
                        Rule::Bindings,
                        30,
                        "separate unrelated or different-kind binding groups",
                    ),
                );
            }
        } else {
            let required = match config.grouping.expressions {
                Expressions::Preserve => false,
                Expressions::Strict => true,
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
                    separate(
                        rule,
                        30,
                        "separate operations with no resolved local grouping relationship",
                    ),
                );
            }
        }
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
