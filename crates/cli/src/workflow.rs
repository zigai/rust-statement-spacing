use crate::{
    Options, Result, VERSION, baseline, cargo, diff, failure, protocol, report, transaction,
    workspace,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::str;

pub(crate) struct Driver {
    pub(crate) options: Options,
    pub(crate) report: Value,
    pub(crate) toolchain: Option<String>,
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
        };
    }

    pub(crate) fn set_report(&mut self, key: &str, value: Value) {
        if let Some(map) = self.report.as_object_mut() {
            map.insert(key.into(), value);
        }
    }

    pub(crate) fn execute(&mut self) -> Result<u8> {
        cargo::setup_cancellation()?;
        let limit = self.options.max_snapshot_mib * 1024 * 1024;
        let exclusions = self
            .options
            .snapshot_exclude
            .iter()
            .map(|name| return workspace::safe_relative(name))
            .collect::<Result<Vec<_>>>()?;
        self.set_report("snapshot_exclusions", json!(exclusions));
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
        workspace::validate_manifest_paths(&root, &exclusions)?;
        if !root.join("Cargo.lock").is_file() {
            return Err(failure(
                "verified mode requires Cargo.lock; run cargo generate-lockfile first",
            ));
        }

        let identity = self.identity(&root)?;
        self.set_report("formatter", identity.clone());
        let original = workspace::scan(&root, limit, &exclusions)?;
        self.set_report("source_fingerprint", source_fingerprint(&original)?.into());
        if !self.options.format_first {
            self.fmt(&root, false)?;
        }

        let temporary = tempfile::Builder::new()
            .prefix("statement-spacing-")
            .tempdir()?;

        let replica = temporary.path().join("workspace");
        workspace::copy_snapshot(&root, &replica, &original, &exclusions)?;
        let replica = workspace::canonical(&replica)?;
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
        if self.options.format_first {
            // Establish declaration groups before rustfmt sorts imports within
            // them. Otherwise removing blank lines destroys the group's order
            // before the whitespace-only lint can reconstruct its boundaries.
            let before_layout = workspace::scan(&replica, limit, &exclusions)?;
            let mut layout = self.lint(&replica, temporary.path(), "declarations", &root)?;
            workspace::assert_snapshot(&replica, &before_layout, limit, &exclusions)?;
            layout
                .edits
                .retain(|edit| return edit.rule == "statement_spacing_item_spacing");
            self.set_report("preformat_declaration_edits", json!(layout.edits.len()));
            self.set_report("preformat_findings", json!(layout.findings));
            protocol::apply(&replica, &layout.edits)?;
            self.fmt(&replica, true)?;
        }

        let before_lint = workspace::scan(&replica, limit, &exclusions)?;
        let mut first = self.lint(&replica, temporary.path(), "initial", &root)?;
        workspace::assert_snapshot(&replica, &before_lint, limit, &exclusions)?;

        let mut features: Vec<_> = self
            .options
            .features
            .iter()
            .flat_map(|value| return value.split(','))
            .collect();
        features.sort_unstable();
        features.dedup();
        let coverage = json!({
            "workspace": true, "all_targets": true, "features": features,
            "all_features": self.options.all_features, "no_default_features": self.options.no_default_features,
            "target": self.options.target,
            "snapshot_exclusions": exclusions,
        });
        self.set_report("coverage", coverage.clone());
        let baseline_document =
            if self.options.baseline.is_some() || self.options.write_baseline.is_some() {
                let fingerprints = baseline::fingerprints(&replica, &first.findings)?;
                if let Some(path) = &self.options.baseline {
                    let matched =
                        baseline::filter(path, &coverage, &fingerprints, &mut first.findings)?;
                    self.set_report("baseline_matched", json!(matched));
                }

                Some(baseline::document(&fingerprints, &coverage))
            } else {
                None
            };

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
        cargo::check_interrupted()?;
        if self.options.command == "check" {
            workspace::assert_snapshot(&root, &original, limit, &exclusions)?;
            if let Some(path) = &self.options.write_baseline {
                let document = baseline_document
                    .as_ref()
                    .ok_or_else(|| return failure("missing baseline document"))?;

                report::write_report(path, document)?;
                self.set_report("baseline_written", json!(path));
                self.set_report("status", "baselined".into());

                return Ok(0);
            }

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
        let candidate = workspace::scan(&replica, limit, &exclusions)?;
        self.fmt(&replica, false)?;
        let second = self.lint(&replica, temporary.path(), "fixed", &root)?;
        workspace::assert_snapshot(&replica, &candidate, limit, &exclusions)?;

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
        if self.options.dry_run {
            workspace::assert_snapshot(&root, &original, limit, &exclusions)?;
            if self.identity(&root)? != identity {
                return Err(failure("formatter identity changed during verification"));
            }

            if self.options.diff {
                let mut patch = String::new();
                for (name, bytes) in &changed {
                    patch.push_str(&diff::unified(
                        name,
                        &fs::read_to_string(root.join(name))?,
                        str::from_utf8(bytes)?,
                    )?);
                }

                self.set_report("diff", patch.into());
            }

            workspace::assert_snapshot(&root, &original, limit, &exclusions)?;

            self.set_report(
                "status",
                if changed.is_empty() {
                    "passed"
                } else {
                    "would-fix"
                }
                .into(),
            );
            self.set_report(
                "verified",
                json!({
                    "rustfmt_candidate": true, "second_lint_run_clean": true,
                    "concurrent_edits_guarded": true, "code_comment_literal_tokens_preserved": true,
                    "rustfmt_original_directory": false,
                }),
            );

            temporary.close()?;

            return Ok(u8::from(!changed.is_empty()));
        }

        let final_check = || {
            if self.identity(&root)? != identity {
                return Err(failure("formatter identity changed during the transaction"));
            }
            return self.fmt(&root, false);
        };

        if changed.is_empty() {
            workspace::assert_snapshot(&root, &original, limit, &exclusions)?;
            let mut final_check = final_check;
            final_check()?;
        } else {
            transaction::commit(&root, &original, &changed, final_check, limit, &exclusions)?;
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
