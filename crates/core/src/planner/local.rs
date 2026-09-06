//! Mandatory joins and group-independent spacing constraints.

use super::decisions::{Decision, blocked, enabled, join, put, separate};
use super::guard_pair;
use crate::config::*;
use crate::model::*;
use crate::relations::{Relationship, direct_producer, relationship};

pub(super) fn apply(
    config: &Config,
    global_rules: RuleMask,
    list: &UnitList,
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
        let long = list.executable_count > config.exits.short_block_max_statements;
        let exit = b.kind == UnitKind::Exit && long;
        let tail = b.is_tail
            && match config.exits.tail {
                Tail::AlwaysSeparate => true,
                Tail::Preserve => false,
                Tail::Smart => long && !direct_producer(&a.facts, &b.facts, &config.grouping),
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
        if a.kind.ends_block() && !guards && config.control_flow.after_block == Separation::Separate
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
