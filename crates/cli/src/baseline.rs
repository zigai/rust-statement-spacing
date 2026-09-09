use crate::{Result, VERSION, failure, workspace};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs;
use std::path::Path;

pub(crate) fn fingerprints(root: &Path, findings: &[Value]) -> Result<Vec<String>> {
    let mut sources = BTreeMap::new();
    let mut result = Vec::with_capacity(findings.len());
    for finding in findings {
        let file = finding
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| return failure("baseline finding lacks file"))?;
        let source = match sources.entry(file) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(fs::read_to_string(
                root.join(workspace::safe_relative(file)?),
            )?),
        };

        let offset = |key: &str| -> Result<usize> {
            return finding
                .get(key)
                .and_then(Value::as_u64)
                .and_then(|value| return usize::try_from(value).ok())
                .ok_or_else(|| return failure("baseline finding lacks source span"));
        };
        let start = offset("byte_start")?;
        let end = offset("byte_end")?;
        let before = source
            .get(..start)
            .ok_or_else(|| return failure("invalid baseline span"))?;
        let body = source
            .get(start..end)
            .ok_or_else(|| return failure("invalid baseline span"))?;
        let after = source
            .get(end..)
            .ok_or_else(|| return failure("invalid baseline span"))?;

        let previous = before
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| return !line.is_empty());

        let next = after
            .lines()
            .map(str::trim)
            .find(|line| return !line.is_empty());

        let anchor = json!([
            file,
            finding["rule"],
            finding["message"],
            previous,
            body.trim(),
            next
        ]);
        result.push(workspace::digest(&serde_json::to_vec(&anchor)?));
    }

    return Ok(result);
}

pub(crate) fn document(fingerprints: &[String], coverage: &Value) -> Value {
    let mut counts = BTreeMap::<&str, usize>::new();
    for fingerprint in fingerprints {
        *counts.entry(fingerprint).or_default() += 1;
    }

    return json!({"schema": 1, "version": VERSION, "coverage": coverage, "findings": counts});
}

pub(crate) fn filter(
    path: &Path,
    coverage: &Value,
    fingerprints: &[String],
    findings: &mut Vec<Value>,
) -> Result<usize> {
    if fs::metadata(path)?.len() > 32 * 1024 * 1024 {
        return Err(failure("baseline exceeds 32 MiB"));
    }

    let baseline: Value = serde_json::from_slice(&fs::read(path)?)?;
    if baseline.get("schema").and_then(Value::as_u64) != Some(1)
        || baseline.get("version").and_then(Value::as_str) != Some(VERSION)
        || baseline.get("coverage") != Some(coverage)
    {
        return Err(failure(
            "baseline schema, version, or Cargo coverage does not match this check",
        ));
    }

    let entries = baseline
        .get("findings")
        .and_then(Value::as_object)
        .ok_or_else(|| return failure("invalid baseline findings"))?;

    let mut counts = BTreeMap::new();
    for (fingerprint, count) in entries {
        let count = count
            .as_u64()
            .filter(|count| return *count != 0)
            .ok_or_else(|| return failure("invalid baseline finding count"))?;
        if fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|byte| return byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(failure("invalid baseline fingerprint"));
        }

        counts.insert(fingerprint, count);
    }

    let original_count = findings.len();
    let mut fingerprints = fingerprints.iter();
    findings.retain(|_| {
        if let Some(fingerprint) = fingerprints.next()
            && let Some(count) = counts.get_mut(fingerprint)
            && *count > 0
        {
            *count -= 1;

            return false;
        }
        return true;
    });

    return Ok(original_count - findings.len());
}
