use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::{Options, Result, VERSION, cargo, failure, protocol, transaction, workspace};

pub(crate) struct Driver {
    pub(crate) options: Options,
    pub(crate) report: Value,
    pub(crate) toolchain: Option<String>,
    pub(crate) package_roots: BTreeMap<String, PathBuf>,
}

impl Driver {
    pub(crate) fn new(options: Options) -> Self {
        let report = json!({
            "schema": 1, "version": VERSION, "mode": options.command,
            "status": "running", "commands": [], "changed_files": [],
        });
        let toolchain = options.format_toolchain.clone();
        return Self {
            options,
            report,
            toolchain,
            package_roots: BTreeMap::new(),
        };
    }

    fn set_report(&mut self, key: &str, value: Value) {
        if let Some(map) = self.report.as_object_mut() {
            map.insert(key.into(), value);
        }
    }

    pub(crate) fn execute(&mut self) -> Result<u8> {
        cargo::setup_cancellation()?;
        let limit = self.options.max_snapshot_mib * 1024 * 1024;
        let root = self.locate()?;
        let _lock = transaction::workspace_lock(&root)?;
        if self.options.command == "recover" {
            self.set_report(
                "recovered_transactions",
                json!(transaction::recover(&root)?),
            );
            self.set_report("status", "recovered".into());
            return Ok(0);
        }
        let unfinished = root.join(".statement-spacing/transactions");
        if unfinished.exists() && fs::read_dir(&unfinished)?.next().transpose()?.is_some() {
            return Err(failure(
                "unfinished transaction found; run cargo statement-spacing recover first",
            ));
        }
        workspace::reject_ancestor_config(&root)?;
        workspace::validate_manifest_paths(&root)?;
        if !root.join("Cargo.lock").is_file() {
            return Err(failure(
                "verified mode requires Cargo.lock; run cargo generate-lockfile first",
            ));
        }
        let identity = self.identity(&root)?;
        self.set_report("formatter", identity.clone());
        let original = workspace::scan(&root, limit)?;
        self.set_report("source_fingerprint", source_fingerprint(&original)?.into());
        if !self.options.format_first {
            self.fmt(&root, false)?;
        }
        let temporary = tempfile::Builder::new()
            .prefix("statement-spacing-")
            .tempdir()?;
        let replica = temporary.path().join("workspace");
        workspace::copy_snapshot(&root, &replica, &original)?;
        let mut args = self.formatter_cargo();
        args.extend([
            "metadata".into(),
            "--no-deps".into(),
            "--locked".into(),
            "--format-version=1".into(),
            "--manifest-path".into(),
            replica.join("Cargo.toml").to_string_lossy().into_owned(),
        ]);
        let metadata = self.run(args, &replica, None, true)?;
        let metadata: Value = serde_json::from_str(&metadata.stdout)?;
        workspace::validate_metadata(&replica, &metadata)?;
        let packages = metadata
            .get("packages")
            .and_then(Value::as_array)
            .ok_or_else(|| return failure("invalid Cargo metadata packages"))?;
        let members = metadata
            .get("workspace_members")
            .and_then(Value::as_array)
            .ok_or_else(|| return failure("invalid Cargo metadata workspace_members"))?;
        for package in packages.iter().filter(|package| {
            return package
                .get("id")
                .is_some_and(|id| return members.contains(id));
        }) {
            let id = package
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| return failure("invalid Cargo package id"))?;
            let manifest = package
                .get("manifest_path")
                .and_then(Value::as_str)
                .ok_or_else(|| return failure("invalid Cargo package manifest_path"))?;
            let parent = Path::new(manifest)
                .parent()
                .ok_or_else(|| return failure("invalid Cargo package manifest_path"))?;
            self.package_roots.insert(id.to_owned(), parent.to_owned());
        }
        if self.options.format_first {
            self.fmt(&replica, true)?;
        }
        let before_lint = workspace::scan(&replica, limit)?;
        let first = self.lint(&replica, temporary.path(), "initial", &root)?;
        workspace::assert_snapshot(&replica, &before_lint, limit)?;
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        for finding in &first.findings {
            let rule = finding
                .get("rule")
                .and_then(Value::as_str)
                .ok_or_else(|| return failure("invalid finding rule"))?;
            *counts.entry(rule).or_default() += 1;
        }
        self.set_report("rule_counts", json!(counts));
        self.set_report("findings", json!(first.findings));
        self.set_report("checked_files", json!(first.checked_files));
        self.set_report("skipped_boundaries", json!(first.skipped_boundaries));
        self.set_report("coverage", json!({
            "workspace": true, "all_targets": true, "features": self.options.features,
            "all_features": self.options.all_features, "no_default_features": self.options.no_default_features,
            "target": self.options.target,
        }));
        cargo::check_interrupted()?;
        if self.options.command == "check" {
            workspace::assert_snapshot(&root, &original, limit)?;
            self.set_report(
                "status",
                if first.findings.is_empty() {
                    "passed"
                } else {
                    "violations"
                }
                .into(),
            );
            return Ok(u8::from(!first.findings.is_empty()));
        }
        if first.findings.iter().any(|finding| {
            return finding.get("fixable").and_then(Value::as_bool) != Some(true);
        }) {
            return Err(failure(
                "at least one finding has no unambiguous machine-applicable fix",
            ));
        }
        protocol::apply(&replica, &first.edits)?;
        let candidate = workspace::scan(&replica, limit)?;
        self.fmt(&replica, false)?;
        let second = self.lint(&replica, temporary.path(), "fixed", &root)?;
        workspace::assert_snapshot(&replica, &candidate, limit)?;
        if !second.findings.is_empty() || !second.edits.is_empty() {
            return Err(failure(
                "candidate is not a lint fixed point; the complete transaction was rejected",
            ));
        }
        if first.checked_files != second.checked_files {
            return Err(failure(
                "compiler coverage changed between the original and candidate runs",
            ));
        }
        if first.token_hashes != second.token_hashes {
            return Err(failure(
                "candidate changed code/comment/literal tokens; transaction rejected",
            ));
        }
        if candidate.keys().ne(original.keys()) {
            return Err(failure(
                "validation created or deleted project files; unsupported path-sensitive build",
            ));
        }
        let mut changed = BTreeMap::new();
        for (relative, state) in &candidate {
            if original.get(relative) != Some(state) {
                if !relative.ends_with(".rs") {
                    return Err(failure(format!(
                        "validation modified a non-Rust project file: {relative}"
                    )));
                }
                changed.insert(relative.clone(), fs::read(replica.join(relative))?);
            }
        }
        self.set_report("proposed_files", json!(changed.keys().collect::<Vec<_>>()));
        cargo::check_interrupted()?;
        let final_check = || {
            if self.identity(&root)? != identity {
                return Err(failure("formatter identity changed during the transaction"));
            }
            return self.fmt(&root, false);
        };
        if changed.is_empty() {
            workspace::assert_snapshot(&root, &original, limit)?;
            let mut final_check = final_check;
            final_check()?;
        } else {
            transaction::commit(&root, &original, &changed, final_check, limit)?;
        }
        self.set_report("changed_files", json!(changed.keys().collect::<Vec<_>>()));
        self.set_report(
            "status",
            if changed.is_empty() {
                "passed"
            } else {
                "fixed"
            }
            .into(),
        );
        self.set_report(
            "verified",
            json!({
                "rustfmt_candidate": true, "rustfmt_original_directory": true,
                "second_lint_run_clean": true, "concurrent_edits_guarded": true,
                "code_comment_literal_tokens_preserved": true,
            }),
        );
        temporary.close()?;
        return Ok(0);
    }
}

// Preserve Python json.dumps(sort_keys=True)'s ASCII and separator encoding: this
// digest is a public report field, not merely an internal snapshot identifier.
fn source_fingerprint(snapshot: &workspace::Snapshot) -> Result<String> {
    let mut encoded = String::from("{");
    for (index, (name, state)) in snapshot.iter().enumerate() {
        if index != 0 {
            encoded.push_str(", ");
        }
        let quoted = serde_json::to_string(name)?;
        for character in quoted.chars() {
            if character.is_ascii() {
                encoded.push(character);
            } else {
                use std::fmt::Write;
                for unit in character.encode_utf16(&mut [0; 2]) {
                    write!(encoded, "\\u{unit:04x}")?;
                }
            }
        }
        encoded.push_str(": \"");
        encoded.push_str(&state.digest);
        encoded.push('"');
    }
    encoded.push('}');
    return Ok(workspace::digest(encoded.as_bytes()));
}
