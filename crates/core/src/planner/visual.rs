//! Visual phases constrained by syntax and affirmative compiler dependencies.

use crate::config::Config;
use crate::model::{Form, Scope, Unit, UnitKind, UnitList};
use crate::relations::{Relationship, intersects, relationship};

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
    return intersects(&a.facts.receivers, &b.facts.receivers, settings)
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

pub(super) fn boundary(config: &Config, source: &str, list: &UnitList, index: usize) -> bool {
    let (Some(a), Some(b)) = (list.units.get(index), list.units.get(index + 1)) else {
        return false;
    };
    let text = |unit: &Unit| return source.get(unit.code_range.as_range()).unwrap_or("");
    let multiline = text(a).contains('\n');
    let producer = intersects(
        &a.facts.definitions,
        &b.facts.reads,
        config.grouping.self_fields,
    );
    if a.shape.form == Form::LetElse {
        return false;
    }
    if a.shape.error_handler.is_some() && a.shape.error_handler == b.shape.error_handler {
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
