//! Whitespace-only edit validation and replacement construction.

use serde::{Deserialize, Serialize};

use crate::model::{ByteRange, Rule};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// One whitespace replacement expressed in original-source coordinates.
pub struct Edit {
    /// Half-open span to replace in the original normalized source.
    pub range: ByteRange,
    /// Exact whitespace required at the span before applying the edit.
    pub expected: String,
    /// Whitespace to insert in place of the expected bytes.
    pub replacement: String,
    /// Spacing check responsible for this replacement.
    pub rule: Rule,
}

/// Applies edits using their original-source coordinates.
///
/// # Errors
/// Rejects invalid UTF-8 boundaries, out-of-bounds or overlapping ranges, stale
/// expected text, and any edit containing non-whitespace bytes.
pub fn apply_edits(source: &str, edits: &[Edit]) -> Result<String, String> {
    let sorted = validate_edits(source, edits)?;
    let mut result = source.to_string();
    for edit in sorted.into_iter().rev() {
        result.replace_range(edit.range.as_range(), &edit.replacement);
    }
    return Ok(result);
}

/// Validates the complete edit set and returns references in source order.
pub(crate) fn validate_edits<'edits>(
    source: &str,
    edits: &'edits [Edit],
) -> Result<Vec<&'edits Edit>, String> {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|edit| return (edit.range.start, edit.range.end));
    let mut previous: Option<&Edit> = None;
    for edit in &sorted {
        let old = source.get(edit.range.as_range()).ok_or_else(|| {
            return "edit is outside source or splits a UTF-8 character".to_string();
        })?;
        if old != edit.expected {
            return Err("stale source: expected trivia differs".into());
        }
        if !old.bytes().all(is_trivia) || !edit.replacement.bytes().all(is_trivia) {
            return Err("edit would modify a non-whitespace byte".into());
        }
        if let Some(prior) = previous
            && (prior.range.end > edit.range.start || prior.range.start == edit.range.start)
        {
            return Err("overlapping edits, including competing insertions".into());
        }
        previous = Some(edit);
    }
    return Ok(sorted);
}

/// Whether a byte is an ASCII space, tab, carriage return, or line feed.
pub fn is_trivia(byte: u8) -> bool {
    return matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
}

/// Change only blank lines. Retain the first line break and following indentation.
/// Refuse compact/same-line layouts rather than inventing a formatter's indentation.
pub fn blank_line_replacement(gap: &str, blank_lines: usize) -> Option<String> {
    if !gap.bytes().all(is_trivia) {
        return None;
    }
    let first = gap.find('\n')?;
    let last = gap.rfind('\n')?;
    let prefix = gap.get(..=first)?;
    let suffix = gap.get(last + 1..)?;
    let mut result = prefix.to_string();
    result.push_str(&"\n".repeat(blank_lines));
    result.push_str(suffix);
    return Some(result);
}
