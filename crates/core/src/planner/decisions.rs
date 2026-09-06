//! Boundary eligibility and priority arbitration shared by every planning phase.

use crate::model::{Rule, RuleMask, Unit, UnitList};

#[derive(Clone, Debug)]
pub(super) struct Decision {
    pub(super) blanks: usize,
    pub(super) rule: Rule,
    pub(super) message: &'static str,
    priority: u8,
    pub(super) anchor: Option<usize>,
}

pub(super) fn enabled(global_rules: RuleMask, unit: &Unit, rule: Rule) -> bool {
    return global_rules.has(rule) && unit.enabled.has(rule);
}

pub(super) fn blocked(list: &UnitList, i: usize) -> bool {
    let Some(gap) = list.gaps.get(i) else {
        return true;
    };
    let Some(unit) = list.units.get(i) else {
        return true;
    };
    let Some(next_unit) = list.units.get(i + 1) else {
        return true;
    };
    return gap.protected
        || unit.protected
        || next_unit.protected
        || !unit.active
        || !next_unit.active;
}

pub(super) fn put(
    global_rules: RuleMask,
    list: &UnitList,
    decisions: &mut [Option<Decision>],
    i: usize,
    decision: Decision,
) -> bool {
    let Some(next_unit) = list.units.get(i + 1) else {
        return false;
    };
    if blocked(list, i) || !enabled(global_rules, next_unit, decision.rule) {
        return false;
    }
    let Some(slot) = decisions.get_mut(i) else {
        return false;
    };
    if slot
        .as_ref()
        .is_some_and(|old| return old.priority >= decision.priority)
    {
        return false;
    }
    *slot = Some(decision);
    return true;
}

pub(super) fn separate(rule: Rule, priority: u8, message: &'static str) -> Decision {
    return Decision {
        blanks: 1,
        rule,
        priority,
        message,
        anchor: None,
    };
}

pub(super) fn join(rule: Rule, message: &'static str) -> Decision {
    return Decision {
        blanks: 0,
        rule,
        priority: 100,
        message,
        anchor: None,
    };
}

pub(super) fn effective_blank(list: &UnitList, decisions: &[Option<Decision>], i: usize) -> usize {
    if let Some(Some(d)) = decisions.get(i) {
        return d.blanks;
    }
    return list.gaps.get(i).map_or(0, |g| return g.blank_lines);
}
