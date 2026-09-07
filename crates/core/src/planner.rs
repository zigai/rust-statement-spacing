//! One owner per boundary, with mandatory joins established before group splitting.

use crate::config::{Config, GuardChain};
use crate::edits::{Edit, blank_line_replacement, validate_edits};
use crate::model::{ByteRange, Rule, RuleMask, SourceModel, Unit, UnitList};
mod decisions;
mod groups;
mod local;
mod setup;

use decisions::{Decision, blocked};

#[derive(Clone, Debug)]
/// One spacing diagnostic, optionally accompanied by a safe whitespace edit.
pub struct Finding {
    /// Check responsible for this diagnostic.
    pub rule: Rule,
    /// Compiler-node identity at which the diagnostic should be emitted.
    pub anchor: usize,
    /// Original normalized-source span of the affected boundary.
    pub range: ByteRange,
    /// Explanation of the required spacing change.
    pub message: String,
    /// Applicable whitespace edit, absent for layouts that cannot be fixed safely.
    pub edit: Option<Edit>,
}

#[derive(Clone, Debug, Default)]
/// Validated set of spacing diagnostics for one source model.
pub struct Plan {
    /// Diagnostics sorted by source span and rule.
    pub findings: Vec<Finding>,
    /// Number of boundaries protected by syntax or inactive code.
    pub skipped_boundaries: usize,
}

impl Plan {
    /// Collects the applicable edits in diagnostic order.
    pub fn edits(&self) -> Vec<Edit> {
        return self
            .findings
            .iter()
            .filter_map(|finding| return finding.edit.clone())
            .collect();
    }
}

fn guard_pair(config: &Config, a: &Unit, b: &Unit) -> bool {
    return config.control_flow.guard_chain == GuardChain::Allow && a.is_guard && b.is_guard;
}

fn build_decisions(
    config: &Config,
    global_rules: RuleMask,
    list: &UnitList,
) -> Vec<Option<Decision>> {
    let mut decisions = vec![None; list.gaps.len()];
    let cohesive = groups::cohesive(config, list);
    local::apply(config, global_rules, list, &cohesive, &mut decisions);
    setup::apply(config, global_rules, list, &cohesive, &mut decisions);
    local::cap_blank_lines(global_rules, list, &mut decisions);
    return decisions;
}

/// Plans spacing changes without mutating the source.
///
/// # Errors
/// Rejects invalid configuration, inconsistent gap/unit counts, invalid source
/// ranges, non-idempotent constraints, or stale, conflicting, and non-trivia edits.
pub fn plan(config: &Config, source: &str, model: &SourceModel) -> Result<Plan, String> {
    config.validate()?;
    let global_rules = config.enabled();
    let mut result = Plan::default();
    for list in &model.lists {
        if list.gaps.len() != list.units.len().saturating_sub(1) {
            return Err("invalid source model: gap/unit count mismatch".into());
        }
        let decisions = build_decisions(config, global_rules, list);
        for (i, decision) in decisions.iter().enumerate() {
            if blocked(list, i) {
                result.skipped_boundaries += 1;
            }
            let Some(decision) = decision else {
                continue;
            };
            let Some(gap) = list.gaps.get(i) else {
                continue;
            };
            if gap.blank_lines == decision.blanks {
                continue;
            }
            let old = source
                .get(gap.range.as_range())
                .ok_or_else(|| return "invalid gap range".to_string())?;
            let edit = if gap.vertical {
                blank_line_replacement(old, decision.blanks).map(|replacement| {
                    return Edit {
                        range: gap.range,
                        expected: old.to_string(),
                        replacement,
                        rule: decision.rule,
                    };
                })
            } else {
                None
            };
            result.findings.push(Finding {
                rule: decision.rule,
                anchor: decision.anchor.unwrap_or_else(|| {
                    return list.units.get(i + 1).map_or(0, |u| return u.anchor);
                }),
                range: gap.range,
                message: decision.message.to_string(),
                edit,
            });
        }
        // The policy on the resulting gap model must be stable, independently of
        // offsets. The source-adapter tests also reparse and check actual edits.
        let mut fixed_model = list.clone();
        for (i, decision) in decisions.iter().enumerate() {
            if let Some(decision) = decision
                && let Some(gap) = fixed_model.gaps.get_mut(i)
            {
                gap.blank_lines = decision.blanks;
            }
        }
        let second = build_decisions(config, global_rules, &fixed_model);
        for (i, decision) in second.iter().enumerate() {
            if let Some(decision) = decision
                && let Some(gap) = fixed_model.gaps.get(i)
                && decision.blanks != gap.blank_lines
            {
                return Err("internal error: spacing constraints are not idempotent".into());
            }
        }
    }
    if global_rules.has(Rule::Layout) {
        for (range, anchor) in &model.layout_edges {
            let old = source
                .get(range.as_range())
                .ok_or("invalid block edge range")?;
            if old.matches('\n').count() <= 1 {
                continue;
            }
            if let Some(replacement) = blank_line_replacement(old, 0) {
                result.findings.push(Finding {
                    rule: Rule::Layout,
                    anchor: *anchor,
                    range: *range,
                    message: "remove empty padding at this block edge".into(),
                    edit: Some(Edit {
                        range: *range,
                        expected: old.to_string(),
                        replacement,
                        rule: Rule::Layout,
                    }),
                });
            }
        }
    }
    result
        .findings
        .sort_by_key(|f| return (f.range.start, f.range.end, f.rule));
    // Validate the whole set, including insertions shared by nested source lists.
    validate_edits(source, &result.edits())?;
    return Ok(result);
}
