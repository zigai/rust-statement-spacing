use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::workspace::{assert_no_symlink, relative_path, safe_relative};
use crate::{Result, failure};

pub(crate) const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Edit {
    pub(crate) relative: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) before: Vec<u8>,
    pub(crate) after: Vec<u8>,
    pub(crate) rule: String,
}

pub(crate) struct LintResult {
    pub(crate) findings: Vec<Value>,
    pub(crate) edits: Vec<Edit>,
    pub(crate) checked_files: Vec<String>,
    pub(crate) skipped_boundaries: u64,
    pub(crate) token_hashes: BTreeMap<String, String>,
}

fn spacing_rule(rule: &str) -> bool {
    return matches!(
        rule.strip_prefix("statement_spacing_"),
        Some(
            "bindings"
                | "expressions"
                | "control_flow"
                | "after_block"
                | "exit"
                | "result_check"
                | "item_spacing"
                | "layout"
        )
    );
}

fn suggestions<'doc>(node: &'doc Value, proposals: &mut Vec<&'doc Value>) {
    if let Some(spans) = node.get("spans").and_then(Value::as_array) {
        for span in spans {
            if span
                .get("suggested_replacement")
                .is_some_and(|replacement| return !replacement.is_null())
                && span.get("suggestion_applicability").and_then(Value::as_str)
                    == Some("MachineApplicable")
            {
                proposals.push(span);
            }
        }
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            suggestions(child, proposals);
        }
    }
}

pub(crate) fn collect_edits(
    root: &Path,
    lines: &str,
) -> Result<(Vec<Value>, Vec<Edit>, Vec<String>)> {
    if lines.len() as u64 > MAX_JSON_BYTES {
        return Err(failure("Cargo JSON output exceeds the verification limit"));
    }
    let mut findings = Vec::new();
    let mut edits = BTreeMap::<(String, usize, usize), Edit>::new();
    let mut errors = Vec::new();
    for line in lines.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let diagnostic = if event.get("reason").and_then(Value::as_str) == Some("compiler-message")
            && event.get("message").is_some_and(Value::is_object)
        {
            let Some(message) = event.get("message") else {
                continue;
            };
            message
        } else if event.get("$message_type").and_then(Value::as_str) == Some("diagnostic") {
            &event
        } else {
            continue;
        };
        let rule = diagnostic
            .get("code")
            .and_then(|code| return code.get("code"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !spacing_rule(rule) {
            if matches!(
                diagnostic.get("level").and_then(Value::as_str),
                Some("error" | "failure-note")
            ) {
                let message = diagnostic
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("compiler error");
                if !message.starts_with("aborting due to")
                    && !message.starts_with("For more information")
                {
                    errors.push(message.to_owned());
                }
            }
            continue;
        }
        // The pinned Cargo compiler runs from the workspace root. Diagnostic
        // file names are relative to that directory, not to the package ID.
        let resolve_name = |name: &str| -> Result<(PathBuf, String)> {
            return relative_path(root, Path::new(name));
        };
        let mut proposals = Vec::new();
        suggestions(diagnostic, &mut proposals);
        let primary = diagnostic
            .get("spans")
            .and_then(Value::as_array)
            .and_then(|spans| {
                return spans.iter().find(|span| {
                    return span.get("is_primary").and_then(Value::as_bool) == Some(true);
                });
            });
        let mut file = primary
            .and_then(|span| return span.get("file_name"))
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(name) = file.as_str().filter(|name| return !name.is_empty()) {
            file = Value::String(resolve_name(name)?.1);
        }
        findings.push(json!({
            "rule": rule,
            "message": diagnostic.get("message").unwrap_or(&Value::String(String::new())),
            "file": file,
            "line": primary.and_then(|span| return span.get("line_start")),
            "fixable": proposals.len() == 1
        }));
        if proposals.len() != 1 {
            continue;
        }
        let Some(&proposal) = proposals.first() else {
            continue;
        };
        let name = proposal
            .get("file_name")
            .and_then(Value::as_str)
            .ok_or_else(|| return failure("invalid suggestion file name"))?;
        let (path, relative) = resolve_name(name)?;
        assert_no_symlink(root, &path)?;
        if !relative.ends_with(".rs") {
            return Err(failure(format!(
                "a statement_spacing suggestion targeted a non-Rust file: {relative}"
            )));
        }
        let data = fs::read(path)?;
        let range = proposal
            .get("byte_start")
            .and_then(Value::as_u64)
            .zip(proposal.get("byte_end").and_then(Value::as_u64))
            .and_then(|(start, end)| {
                return Some((usize::try_from(start).ok()?, usize::try_from(end).ok()?));
            });
        let (start, end) = range
            .filter(|&(start, end)| return start <= end && end <= data.len())
            .ok_or_else(|| {
                return failure(format!("invalid suggestion byte range in {relative}"));
            })?;
        let Some(slice) = data.get(start..end) else {
            return Err(failure(format!(
                "invalid suggestion byte range in {relative}"
            )));
        };
        let before = slice.to_vec();
        let after = proposal
            .get("suggested_replacement")
            .and_then(Value::as_str)
            .ok_or_else(|| return failure("invalid suggestion replacement"))?
            .as_bytes()
            .to_vec();
        if before
            .iter()
            .chain(&after)
            .any(|byte| return !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            return Err(failure(format!(
                "non-whitespace suggestion was rejected in {relative}"
            )));
        }
        let key = (relative.clone(), start, end);
        if let Some(previous) = edits.get(&key)
            && (previous.before != before || previous.after != after)
        {
            return Err(failure(format!(
                "conflicting target-specific suggestions in {relative}"
            )));
        }
        edits.insert(
            key,
            Edit {
                relative,
                start,
                end,
                before,
                after,
                rule: rule.to_owned(),
            },
        );
    }
    let ordered: Vec<_> = edits.into_values().collect();
    for pair in ordered.windows(2) {
        let [previous, current] = pair else {
            continue;
        };
        if previous.relative == current.relative
            && (previous.end > current.start || previous.start == current.start)
        {
            return Err(failure(format!(
                "overlapping suggestions in {}",
                current.relative
            )));
        }
    }
    return Ok((findings, ordered, errors));
}

pub(crate) fn apply(root: &Path, edits: &[Edit]) -> Result<()> {
    let mut grouped = BTreeMap::<&str, Vec<&Edit>>::new();
    for edit in edits {
        grouped.entry(&edit.relative).or_default().push(edit);
    }
    for (name, mut group) in grouped {
        let path = root.join(safe_relative(name)?);
        assert_no_symlink(root, &path)?;
        let mut data = fs::read(&path)?;
        group.sort_by_key(|edit| return Reverse(edit.start));
        for edit in group {
            if data.get(edit.start..edit.end) != Some(edit.before.as_slice()) {
                return Err(failure(format!("stale suggestion in {name}")));
            }
            data.splice(edit.start..edit.end, edit.after.iter().copied());
        }
        fs::write(path, data)?;
    }
    return Ok(());
}
