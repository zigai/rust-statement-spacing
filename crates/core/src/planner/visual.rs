//! Visual phases constrained by syntax and affirmative compiler dependencies.

use crate::config::Config;
use crate::model::{Form, Scope, StatementRole, Unit, UnitKind, UnitList};
use crate::relations::{Relationship, intersects, relationship};

/// Test assertions form phases, but an immediate operation/check remains one step.
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

        return Some(
            count > config.exits.short_block_max_statements && !operation_check && !scenario_check,
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

/// A short uninterrupted setup can prepare distinct fixtures for one check.
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

pub(super) fn compact_tail(list: &UnitList, a: &Unit, b: &Unit) -> bool {
    if a.kind == UnitKind::Control && matches!(a.shape.form, Form::Conditional | Form::CallCheck) {
        return guard_continuation(a, b);
    }
    return (list.scope == Scope::Closure
        && list.executable_count == 2
        && a.kind == UnitKind::Control)
        || (b.is_tail
            && b.kind == UnitKind::Expression
            && matches!(b.shape.form, Form::Value | Form::Literal))
        || (b.is_tail && b.kind == UnitKind::Control && b.is_guard)
        || (list.executable_count == 2
            && a.kind == UnitKind::Let
            && matches!(b.shape.form, Form::Branch | Form::Conditional))
        || (list.executable_count == 2
            && a.shape.form == Form::Loop
            && b.shape.form == Form::Macro);
}

fn mutation(unit: &Unit) -> bool {
    return !unit.facts.writes.is_empty() || !unit.facts.mutating_receivers.is_empty();
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

/// Affirmative visual connections, not the absence of a reason to separate.
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
                && ((short_a && short_b)
                    || (a.facts.definitions.len() > 1 && short_b)
                    || (b.shape.error_handler.is_some()
                        && source
                            .get(b.code_range.as_range())
                            .is_some_and(|text| return text.lines().count() <= 2))))))
        || (short_a
            && short_b
            && a.kind.is_binding()
            && b.kind.is_binding()
            && relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Related)
        || (short_a
            && short_b
            && a.kind == UnitKind::Expression
            && b.kind == UnitKind::Expression
            && a.shape.role == StatementRole::FunctionCall
            && b.shape.role == StatementRole::FunctionCall
            && a.shape.fallible
            && b.shape.fallible
            && !a.facts.direct_callees.is_empty()
            && !b.facts.direct_callees.is_empty())
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
        || (a.kind.ordinary() && b.kind.ordinary() && resource_pair(config, a, b))
        || (a.kind == UnitKind::Assignment
            && !mutation(b)
            && intersects(&a.facts.reads, &b.facts.reads, settings));
}

pub(super) fn boundary(config: &Config, source: &str, list: &UnitList, index: usize) -> bool {
    let (Some(a), Some(b)) = (list.units.get(index), list.units.get(index + 1)) else {
        return false;
    };

    let text = |unit: &Unit| return source.get(unit.code_range.as_range()).unwrap_or("");
    let multiline = text(a).contains('\n');
    // A shared mutable input does not collapse selection, construction, and
    // conditional configuration into a single visual phase.
    if a.kind.is_binding()
        && multiline
        && ((matches!(a.shape.form, Form::Conditional | Form::Branch)
            && b.kind.is_binding()
            && text(b).contains('\n'))
            || (b.kind == UnitKind::Control
                && matches!(b.shape.form, Form::Conditional | Form::CallCheck)
                && !b.shape.exiting_guard))
    {
        return true;
    }

    if config.grouping.join_related && related_join(config, source, a, b) {
        return false;
    }

    let producer = intersects(
        &a.facts.definitions,
        &b.facts.reads,
        config.grouping.self_fields,
    );

    if a.shape.form == Form::LetElse && producer {
        return false;
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
        return false;
    }

    if producer
        && ((a.shape.form == Form::Value && b.kind.is_binding())
            || a.shape.form == Form::Closure
            || b.kind == UnitKind::Assignment)
    {
        return false;
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
        return false;
    }

    if b.kind.ordinary()
        && (intersects(&a.facts.writes, &b.facts.reads, config.grouping.self_fields)
            || intersects(
                &a.facts.mutating_receivers,
                &b.facts.reads,
                config.grouping.self_fields,
            ))
    {
        return false;
    }

    if sandwiched(config, list, index) || sandwiched(config, list, index + 1) {
        return false;
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
        return false;
    }

    if a.kind == UnitKind::Expression
        && b.kind == UnitKind::Control
        && intersects(
            &a.facts.captures,
            &b.facts.header_reads,
            config.grouping.self_fields,
        )
    {
        return false;
    }

    if producer && !text(b).contains('\n') && text(a).lines().count() <= 2 && b.kind.is_binding() {
        return false;
    }

    if producer && mutation(b) {
        return false;
    }

    if producer
        && b.kind.is_binding()
        && b.shape.error_handler.is_some()
        && text(b).lines().count() <= 2
    {
        return false;
    }

    if producer && a.facts.definitions.len() > 1 && b.kind.is_binding() && !text(b).contains('\n') {
        return false;
    }

    if a.kind == UnitKind::Assignment
        && !mutation(b)
        && intersects(&a.facts.reads, &b.facts.reads, config.grouping.self_fields)
    {
        return false;
    }

    if multiline {
        return true;
    }

    if a.kind == UnitKind::Let
        && b.kind == UnitKind::Control
        && !b.shape.exiting_guard
        && b.shape.form != Form::Loop
        && !producer
    {
        return true;
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
        return true;
    }

    if a.kind == UnitKind::Expression && b.kind == UnitKind::Let {
        if same_string(source, a, b) {
            return false;
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
            return false;
        }

        if !mutation(a)
            && (intersects(&a.facts.reads, &b.facts.reads, config.grouping.self_fields)
                || (!a.facts.reads.is_empty()
                    && !b.shape.fallible
                    && !matches!(b.shape.form, Form::Branch | Form::Conditional)
                    && !text(b).contains('\n')))
        {
            return false;
        }
        return !producer;
    }

    if a.kind == UnitKind::Expression && b.kind == UnitKind::Expression {
        return relationship(&a.facts, &b.facts, &config.grouping) == Relationship::Unrelated;
    }
    return false;
}
