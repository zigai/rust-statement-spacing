//! Visual phases constrained by syntax and affirmative compiler dependencies.

use crate::config::Config;
use crate::model::{Form, Scope, StatementRole, Unit, UnitKind, UnitList};
use crate::relations::{Relationship, header_covered_by, intersects, relationship};
use std::collections::BTreeSet;

pub(super) fn assertion_boundary(config: &Config, list: &UnitList, index: usize) -> Option<bool> {
    let a = list.units.get(index)?;
    let b = list.units.get(index + 1)?;
    let count = a.shape.test_statements?;

    if !a.kind.ordinary() || !b.kind.ordinary() {
        return None;
    }

    if a.shape.role == StatementRole::Assertion && b.shape.role == StatementRole::Assertion {
        return Some(false);
    }

    if b.shape.role == StatementRole::Assertion {
        let operation_check = a.kind == UnitKind::Let
            && a.shape.role != StatementRole::Extraction
            && a.shape.error_handler.is_none()
            && matches!(
                a.shape.form,
                Form::Call | Form::Method | Form::Value | Form::Literal
            )
            && intersects(
                &a.facts.definitions,
                &b.facts.reads,
                config.grouping.self_fields,
            );
        let scenario_check = mutation_check(config, a, b)
            || (call_check(config, a, b)
                && list
                    .units
                    .windows(2)
                    .filter(|pair| {
                        let [action, check] = pair else {
                            return false;
                        };

                        return call_check(config, action, check);
                    })
                    .count()
                    > 1);

        let reexamination = a.kind == UnitKind::Expression
            && header_covered_by(&a.facts.reads, &b.facts.reads, config.grouping.self_fields)
            && !header_covered_by(&b.facts.reads, &a.facts.reads, config.grouping.self_fields);
        return Some(
            count > config.exits.short_block_max_statements
                && (!operation_check || b.shape.pins_value)
                && (!scenario_check || b.shape.pins_value)
                && !reexamination,
        );
    }

    if a.shape.role == StatementRole::Assertion {
        let verification = b.shape.role == StatementRole::FunctionCall
            && index
                .checked_sub(1)
                .and_then(|previous| return list.units.get(previous))
                .is_some_and(|operation| {
                    return operation.kind == UnitKind::Let
                        && operation.shape.role != StatementRole::Extraction
                        && intersects(
                            &operation.facts.definitions,
                            &a.facts.reads,
                            config.grouping.self_fields,
                        )
                        && intersects(
                            &operation.facts.reads,
                            &b.facts.reads,
                            config.grouping.self_fields,
                        );
                });

        if verification {
            return Some(false);
        }
        return Some(count > config.exits.short_block_max_statements);
    }

    if b.shape.role == StatementRole::Extraction
        && list
            .units
            .get(index + 2)
            .is_some_and(|next| return next.shape.role == StatementRole::Assertion)
        && intersects(
            &a.facts.definitions,
            &b.facts.reads,
            config.grouping.self_fields,
        )
    {
        return Some(false);
    }

    if fixture_preparation(config, list, index) {
        return Some(false);
    }
    return None;
}

fn mutation_check(config: &Config, action: &Unit, check: &Unit) -> bool {
    return action.kind.ordinary()
        && action.shape.role != StatementRole::Assertion
        && check.shape.role == StatementRole::Assertion
        && (intersects(
            &action.facts.writes,
            &check.facts.reads,
            config.grouping.self_fields,
        ) || intersects(
            &action.facts.mutating_receivers,
            &check.facts.reads,
            config.grouping.self_fields,
        ));
}

fn call_check(config: &Config, action: &Unit, check: &Unit) -> bool {
    return action.kind == UnitKind::Expression
        && action.shape.role == StatementRole::FunctionCall
        && check.shape.role == StatementRole::Assertion
        && intersects(
            &action.facts.reads,
            &check.facts.reads,
            config.grouping.self_fields,
        );
}

fn fixture_preparation(config: &Config, list: &UnitList, index: usize) -> bool {
    let following = list.units.iter().skip(index).take(5);
    let Some(check) = following
        .clone()
        .find(|unit| return unit.shape.role == StatementRole::Assertion)
    else {
        return false;
    };

    return following
        .take_while(|unit| return unit.shape.role != StatementRole::Assertion)
        .all(|unit| {
            return mutation_check(config, unit, check)
                || (unit.kind == UnitKind::Let
                    && !matches!(
                        unit.shape.form,
                        Form::Branch | Form::Conditional | Form::Closure | Form::LetElse
                    )
                    && unit.shape.role != StatementRole::Extraction
                    && intersects(
                        &unit.facts.definitions,
                        &check.facts.reads,
                        config.grouping.self_fields,
                    ));
        });
}

pub(super) fn validation_stages(list: &UnitList) -> bool {
    return list
        .units
        .iter()
        .filter(|unit| return unit.is_guard)
        .count()
        > 1
        && list.units.iter().all(|unit| {
            return unit.is_guard
                || unit.kind == UnitKind::Exit
                || unit.kind == UnitKind::Expression;
        });
}

pub(super) fn guard_continuation(a: &Unit, b: &Unit) -> bool {
    return a.kind == UnitKind::Control
        && matches!(a.shape.form, Form::Conditional | Form::CallCheck)
        && a.is_guard
        && a.shape.exiting_guard
        && b.kind == UnitKind::Exit
        && !b.is_loop_exit;
}

pub(super) fn compact_tail(
    config: &Config,
    source: &str,
    list: &UnitList,
    a: &Unit,
    b: &Unit,
) -> bool {
    if a.kind == UnitKind::Control && matches!(a.shape.form, Form::Conditional | Form::CallCheck) {
        return guard_continuation(a, b);
    }
    if b.kind == UnitKind::Exit
        && a.kind == UnitKind::Control
        && a.shape.form == Form::Loop
        && source
            .get(b.code_range.as_range())
            .is_none_or(|text| return !text.contains('\n'))
        && source.get(a.code_range.as_range()).is_some_and(|text| {
            return text.trim_end().ends_with('}') && !text.trim_end().ends_with("};");
        })
        && (!b.facts.reads.is_empty()
            || source
                .get(a.code_range.as_range())
                .is_none_or(|text| return !text.contains("&mut")))
    {
        return true;
    }
    if (list.scope == Scope::Closure && list.executable_count == 2 && a.kind == UnitKind::Control)
        || (b.is_tail
            && b.kind == UnitKind::Expression
            && matches!(b.shape.form, Form::Value | Form::Literal))
        || (b.is_tail && b.kind == UnitKind::Control && b.is_guard)
        || (list.executable_count == 2
            && a.kind == UnitKind::Let
            && matches!(b.shape.form, Form::Branch | Form::Conditional))
        || (list.executable_count == 2 && a.shape.form == Form::Loop && b.shape.form == Form::Macro)
    {
        return true;
    }
    if b.kind != UnitKind::Exit && !b.is_tail {
        return false;
    }
    let b_text = source.get(b.code_range.as_range()).unwrap_or("");
    let b_single = !b_text.contains('\n');
    let a_text = source.get(a.code_range.as_range()).unwrap_or("");
    let producer = intersects(
        &a.facts.definitions,
        &b.facts.reads,
        config.grouping.self_fields,
    );
    if b.kind == UnitKind::Exit && b_single && producer {
        return true;
    }
    if b.kind == UnitKind::Exit
        && a.kind == UnitKind::Let
        && !a_text.contains('\n')
        && producer
        && b.shape.form == Form::Method
        && b.facts.reads.len() >= 2
    {
        return true;
    }
    if b.kind == UnitKind::Exit
        && b_single
        && b.facts.reads.is_empty()
        && (a_text.trim_end().ends_with("});")
            || (a.shape.form == Form::Macro && a.shape.role != StatementRole::Assertion))
    {
        return true;
    }
    if b.kind == UnitKind::Exit
        && b_single
        && !a.kind.ends_block()
        && !producer
        && a_text.lines().count() <= 8
        && (!a.facts.reads.is_empty() || !b.facts.reads.is_empty())
    {
        return true;
    }
    if b.kind == UnitKind::Exit
        && !b_single
        && list.executable_count <= config.exits.short_block_max_statements
        && !list.units.iter().any(|unit| {
            return intersects(
                &unit.facts.definitions,
                &b.facts.reads,
                config.grouping.self_fields,
            );
        })
    {
        return true;
    }
    return false;
}

fn mutation(unit: &Unit) -> bool {
    return !unit.facts.writes.is_empty() || !unit.facts.mutating_receivers.is_empty();
}

pub(super) fn is_validation_adapter(callee: &str) -> bool {
    let name = callee
        .rsplit([':', '.', '#', '/'])
        .find(|segment| return !segment.is_empty())
        .unwrap_or(callee);
    return matches!(
        name,
        "expect"
            | "expect_err"
            | "unwrap"
            | "unwrap_or"
            | "unwrap_or_default"
            | "unwrap_or_else"
            | "map_err"
            | "ok_or"
            | "ok_or_else"
            | "or"
            | "or_else"
            | "into_diagnostic"
            | "into_report"
    );
}
fn has_macro_bang(text: &str) -> bool {
    return text.replace("!=", "").contains("!(")
        || text.replace("!=", "").contains("![")
        || text.replace("!=", "").contains("!{");
}
pub(super) fn bool_flag_run(source: &str, a: &Unit, b: &Unit) -> bool {
    fn first_line_has_bool_op(text: &str) -> bool {
        return text.lines().next().is_some_and(|first| {
            return first.contains("&&")
                || first.contains("==")
                || first.contains("!=")
                || first.contains(">=")
                || first.contains("<=");
        });
    }
    fn plain_step(form: Form) -> bool {
        return matches!(
            form,
            Form::Other | Form::Value | Form::Call | Form::Method | Form::Literal
        );
    }
    return b.kind == UnitKind::Let
        && plain_step(b.shape.form)
        && a.kind.ordinary()
        && plain_step(a.shape.form)
        && source.get(b.code_range.as_range()).is_some_and(|text| {
            return first_line_has_bool_op(text) && !text.contains("||") && !has_macro_bang(text);
        });
}

fn resource_pair(config: &Config, a: &Unit, b: &Unit) -> bool {
    let settings = config.grouping.self_fields;

    return (a.shape.form == Form::Method
        && b.shape.form == Form::Method
        && intersects(&a.facts.receivers, &b.facts.receivers, settings))
        || intersects(&a.facts.writes, &b.facts.writes, settings)
        || intersects(
            &a.facts.mutating_receivers,
            &b.facts.mutating_receivers,
            settings,
        );
}

fn sandwiched(config: &Config, list: &UnitList, index: usize) -> bool {
    let Some(unit) = list.units.get(index) else {
        return false;
    };
    if unit.kind != UnitKind::Let {
        return false;
    }
    return list
        .units
        .iter()
        .take(index)
        .rev()
        .take_while(|unit| return unit.kind == UnitKind::Expression)
        .any(|before| {
            return list
                .units
                .iter()
                .skip(index + 1)
                .take_while(|unit| return unit.kind == UnitKind::Expression)
                .any(|after| return resource_pair(config, before, after));
        });
}

fn same_string(source: &str, a: &Unit, b: &Unit) -> bool {
    return a.shape.string_inputs.iter().any(|a| {
        return b.shape.string_inputs.iter().any(|b| {
            return source
                .get(a.as_range())
                .is_some_and(|a| return Some(a) == source.get(b.as_range()));
        });
    });
}

pub(super) fn related_join(config: &Config, source: &str, a: &Unit, b: &Unit) -> bool {
    let settings = config.grouping.self_fields;
    let producer = intersects(&a.facts.definitions, &b.facts.reads, settings);
    let short_a = source
        .get(a.code_range.as_range())
        .is_some_and(|text| return text.lines().count() <= 2);
    let short_b = source
        .get(b.code_range.as_range())
        .is_some_and(|text| return !text.contains('\n'));

    return (producer
        && (a.shape.form == Form::LetElse
            || a.shape.form == Form::Closure
            || (a.shape.form == Form::Value && b.kind.is_binding())
            || b.kind == UnitKind::Assignment
            || mutation(b)
            || (short_a && short_b && b.kind.ordinary())
            || (b.kind.is_binding()
                && (((short_a || a.facts.definitions.len() > 1) && short_b)
                    || (b.shape.error_handler.is_some()
                        && source
                            .get(b.code_range.as_range())
                            .is_some_and(|text| return text.lines().count() <= 2))))))
        || ({
            let short_pair = source
                .get(a.code_range.as_range())
                .is_some_and(|text| return !text.contains('\n'))
                && source
                    .get(b.code_range.as_range())
                    .is_some_and(|text| return !text.contains('\n'));
            let same_inputs = !b.facts.reads.is_empty()
                && header_covered_by(&a.facts.reads, &b.facts.reads, config.grouping.self_fields);
            let input_roots = |unit: &Unit| {
                return unit
                    .facts
                    .reads
                    .iter()
                    .chain(unit.facts.receivers.iter())
                    .map(|place| return place.local.clone())
                    .collect::<BTreeSet<_>>();
            };
            let root_shared = !input_roots(a).is_disjoint(&input_roots(b));
            let shared_operation = config.grouping.same_callee
                && a.facts.direct_callees.iter().any(|callee| {
                    return !is_validation_adapter(callee)
                        && b.facts.direct_callees.contains(callee);
                });
            (short_pair || same_inputs || root_shared)
                && (a.kind == UnitKind::Expression || a.kind.is_binding())
                && (b.kind == UnitKind::Expression || b.kind.is_binding())
                && shared_operation
        })
        || (short_a
            && short_b
            && a.kind.is_binding()
            && b.kind.is_binding()
            && relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Related)
        || (a.kind.is_binding()
            && b.kind.is_binding()
            && a.shape.error_handler.is_some()
            && a.shape.error_handler == b.shape.error_handler
            && (producer
                || intersects(&a.facts.reads, &b.facts.reads, settings)
                // Shared callback captures connect these parallel computations;
                // they are not evidence that either callback has executed.
                || intersects(&a.facts.captures, &b.facts.captures, settings)))
        || (a.shape.mutable
            && (intersects(&a.facts.definitions, &b.facts.writes, settings)
                || intersects(&a.facts.definitions, &b.facts.mutating_receivers, settings)))
        || (b.kind.ordinary()
            && (intersects(&a.facts.writes, &b.facts.reads, settings)
                || intersects(&a.facts.mutating_receivers, &b.facts.reads, settings)))
        || (a.kind.ordinary() && b.kind.ordinary() && resource_pair(config, a, b));
}

pub(super) fn boundary(
    config: &Config,
    source: &str,
    list: &UnitList,
    index: usize,
) -> Option<&'static str> {
    let (Some(a), Some(b)) = (list.units.get(index), list.units.get(index + 1)) else {
        return None;
    };

    let text = |unit: &Unit| return source.get(unit.code_range.as_range()).unwrap_or("");
    let multiline = text(a).contains('\n');
    let match_continuation = a.shape.form == Form::Branch
        && b.kind.is_binding()
        && b.shape.form == Form::Method
        && text(b).contains('\n')
        && (intersects(
            &a.facts.definitions,
            &b.facts.reads,
            config.grouping.self_fields,
        ) || intersects(
            &a.facts.definitions,
            &b.facts.receivers,
            config.grouping.self_fields,
        ));
    fn arms_do_work(text: &str) -> bool {
        let Some((_, arms)) = text.split_once('{') else {
            return false;
        };
        if has_macro_bang(arms) {
            return true;
        }
        fn is_ident(mark: Option<&u8>) -> bool {
            return mark.is_some_and(|byte| return byte.is_ascii_alphanumeric() || *byte == b'_');
        }
        let bytes = arms.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !is_ident(bytes.get(i)) {
                i += 1;
                continue;
            }
            let start = i;
            while is_ident(bytes.get(i)) {
                i += 1;
            }
            let Some(token) = arms.get(start..i) else {
                continue;
            };
            let Some(first) = token.chars().next() else {
                continue;
            };
            let bounded = start == 0 || !is_ident(bytes.get(start - 1));
            if !bounded || (!first.is_lowercase() && first != '_') {
                continue;
            }
            let mut end = i;
            while bytes
                .get(end)
                .is_some_and(|mark| return mark.is_ascii_whitespace())
            {
                end += 1;
            }
            if bytes.get(end).is_some_and(|mark| return *mark == b'(')
                && !is_validation_adapter(token)
            {
                return true;
            }
        }
        return false;
    }
    let dispatch_run = b.kind == UnitKind::Let
        && matches!(b.shape.form, Form::Conditional | Form::Branch)
        && a.kind == UnitKind::Let
        && matches!(a.shape.form, Form::Conditional | Form::Branch)
        && !b.facts.header_reads.is_empty()
        && (!intersects(
            &a.facts.header_reads,
            &b.facts.header_reads,
            config.grouping.self_fields,
        ) || !arms_do_work(text(b)));
    if a.kind.is_binding()
        && multiline
        && ((matches!(a.shape.form, Form::Conditional | Form::Branch)
            && b.kind.is_binding()
            && text(b).contains('\n')
            && !match_continuation
            && !dispatch_run)
            || (b.kind == UnitKind::Control
                && matches!(b.shape.form, Form::Conditional | Form::CallCheck)
                && !b.shape.exiting_guard))
    {
        return Some("separate conditional configuration from its production");
    }

    if config.grouping.join_related && related_join(config, source, a, b) {
        return None;
    }

    let producer = intersects(
        &a.facts.definitions,
        &b.facts.reads,
        config.grouping.self_fields,
    );

    if a.shape.form == Form::LetElse && producer {
        return None;
    }

    if a.shape.error_handler.is_some()
        && a.shape.error_handler == b.shape.error_handler
        && (producer
            || intersects(&a.facts.reads, &b.facts.reads, config.grouping.self_fields)
            || intersects(
                &a.facts.captures,
                &b.facts.captures,
                config.grouping.self_fields,
            ))
    {
        return None;
    }

    if producer
        && ((a.shape.form == Form::Value && b.kind.is_binding())
            || a.shape.form == Form::Closure
            || b.kind == UnitKind::Assignment)
    {
        return None;
    }

    if a.shape.mutable
        && (intersects(
            &a.facts.definitions,
            &b.facts.writes,
            config.grouping.self_fields,
        ) || intersects(
            &a.facts.definitions,
            &b.facts.mutating_receivers,
            config.grouping.self_fields,
        ))
    {
        return None;
    }

    if b.kind.ordinary()
        && (intersects(&a.facts.writes, &b.facts.reads, config.grouping.self_fields)
            || intersects(
                &a.facts.mutating_receivers,
                &b.facts.reads,
                config.grouping.self_fields,
            ))
    {
        return None;
    }
    if sandwiched(config, list, index) || sandwiched(config, list, index + 1) {
        return None;
    }
    let b_lines = text(b).lines().count();
    fn outermost_default(text: &str) -> bool {
        let mut position: Option<usize> = None;
        for marker in ["unwrap_or(", "unwrap_or_default(", "unwrap_or_else("] {
            if let Some(at) = text.rfind(marker) {
                let end = at.saturating_add(marker.len());
                position = Some(position.map_or(end, |best| return best.max(end)));
            }
        }
        let Some(at) = position else {
            return false;
        };
        let Some(mut rest) = text.get(at..).map(|tail| return tail.trim_start()) else {
            return false;
        };
        let mut depth = 0_usize;
        for (index, mark) in rest.char_indices() {
            if mark == '(' {
                depth += 1;
            } else if mark == ')' {
                if depth == 0 {
                    let Some(tail) = rest.get(index + 1..) else {
                        return false;
                    };
                    rest = tail.trim_start();
                    rest = rest.strip_prefix('?').unwrap_or(rest).trim_start();
                    return rest.is_empty() || rest == ";";
                }
                depth = depth.saturating_sub(1);
            }
        }
        return false;
    }
    let produced_use = b.kind.is_binding()
        && (!matches!(a.shape.form, Form::Branch | Form::Conditional | Form::Loop)
            || (a.shape.form == Form::Branch && b.shape.form == Form::Method && b_lines > 1))
        && (b_lines <= 1 || (b_lines <= 5 && b.shape.form == Form::Method))
        && matches!(b.shape.form, Form::Call | Form::Method)
        && !outermost_default(text(b))
        && (intersects(
            &a.facts.definitions,
            &b.facts.reads,
            config.grouping.self_fields,
        ) || intersects(
            &a.facts.definitions,
            &b.facts.receivers,
            config.grouping.self_fields,
        ) || intersects(
            &a.facts.definitions,
            &b.facts.mutating_receivers,
            config.grouping.self_fields,
        ));
    let dispatch_refinement = b.kind == UnitKind::Let
        && matches!(b.shape.form, Form::Conditional | Form::Branch)
        && !matches!(a.shape.form, Form::Branch | Form::Conditional | Form::Loop)
        && !outermost_default(text(b))
        && !b.facts.reads.is_empty()
        && header_covered_by(
            &a.facts.definitions,
            &b.facts.reads,
            config.grouping.self_fields,
        );
    let bool_run = bool_flag_run(source, a, b);
    let parallel_run = config.grouping.same_callee
        && b.kind == UnitKind::Let
        && a.kind == UnitKind::Let
        && a.facts.direct_callees.iter().any(|callee| {
            return !is_validation_adapter(callee) && b.facts.direct_callees.contains(callee);
        })
        && a.shape.error_handler == b.shape.error_handler
        && !intersects(
            &a.facts.definitions,
            &b.facts.reads,
            config.grouping.self_fields,
        )
        && !intersects(
            &a.facts.definitions,
            &b.facts.receivers,
            config.grouping.self_fields,
        );
    if multiline
        && !produced_use
        && !dispatch_refinement
        && !bool_run
        && !parallel_run
        && !dispatch_run
    {
        return Some("separate a new phase after this multiline production");
    }
    if list.scope == Scope::Loop
        && b.is_guard
        && (list.executable_count <= 4
            || producer
            || intersects(
                &a.facts.definitions,
                &b.facts.captures,
                config.grouping.self_fields,
            ))
    {
        return None;
    }

    if producer && !text(b).contains('\n') && text(a).lines().count() <= 2 && b.kind.is_binding() {
        return None;
    }

    if producer && mutation(b) {
        return None;
    }

    if producer
        && b.kind.is_binding()
        && b.shape.error_handler.is_some()
        && text(b).lines().count() <= 2
    {
        return None;
    }

    if producer && a.facts.definitions.len() > 1 && b.kind.is_binding() && !text(b).contains('\n') {
        return None;
    }
    if multiline
        && !produced_use
        && !dispatch_refinement
        && !bool_run
        && !parallel_run
        && !dispatch_run
    {
        return Some("separate a new phase after this multiline production");
    }
    if a.kind == UnitKind::Let
        && b.kind == UnitKind::Control
        && !b.shape.exiting_guard
        && b.shape.form != Form::Loop
        && !producer
    {
        return Some("separate this control flow from its setup");
    }

    if a.kind.is_binding()
        && b.kind == UnitKind::Expression
        && !producer
        && index > 0
        && let Some(previous) = list.units.get(index - 1)
        && text(previous).contains('\n')
        && intersects(
            &previous.facts.definitions,
            &a.facts.reads,
            config.grouping.self_fields,
        )
        && intersects(
            &previous.facts.definitions,
            &b.facts.reads,
            config.grouping.self_fields,
        )
    {
        return Some("separate this operation from earlier multiline setup");
    }

    if a.kind == UnitKind::Expression && b.kind == UnitKind::Let {
        if same_string(source, a, b) {
            return None;
        }

        if !mutation(a)
            && !matches!(b.shape.form, Form::Branch | Form::Conditional)
            && index > 0
            && let Some(previous) = list.units.get(index - 1)
            && previous.kind == UnitKind::Let
            && intersects(
                &previous.facts.definitions,
                &a.facts.reads,
                config.grouping.self_fields,
            )
            && intersects(
                &previous.facts.reads,
                &b.facts.reads,
                config.grouping.self_fields,
            )
        {
            return None;
        }

        if !mutation(a)
            && (intersects(&a.facts.reads, &b.facts.reads, config.grouping.self_fields)
                || (matches!(b.shape.form, Form::Literal | Form::Value)
                    && !b.shape.fallible
                    && !text(b).contains('\n')))
        {
            return None;
        }
        if producer {
            return None;
        }
        return Some("separate this binding from its operation");
    }

    if a.kind.is_binding() && b.kind == UnitKind::Expression {
        if relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Unrelated
            && !related_join(config, source, a, b)
        {
            return Some("separate these operation phases");
        }
        return None;
    }

    if a.kind == UnitKind::Expression && b.kind == UnitKind::Expression {
        if relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Unrelated {
            return Some("separate these operation phases");
        }
        return None;
    }
    return None;
}
