"""Native CLI behavior with explicit Cargo process doubles, not compiler proof."""

try:
    import fcntl
except ImportError:
    fcntl = None
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

if sys.platform == "win32":
    import msvcrt

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_BINARY = ROOT / "target/debug" / (
    "cargo-statement-spacing.exe" if sys.platform == "win32" else "cargo-statement-spacing"
)
BINARY = Path(os.environ.get("STATEMENT_SPACING_BIN", DEFAULT_BINARY)).resolve()
if sys.platform == "win32" and not BINARY.suffix:
    candidate = BINARY.with_suffix(".exe")
    if candidate.exists():
        BINARY = candidate
FAKE_CARGO = Path(__file__).parent / "support/fake_cargo.py"
SOURCE = b"pub fn sample() {\n    let _a = 1;\n    std::hint::black_box(2);\n}\n"
FIXED = SOURCE.replace(b"1;\n", b"1;\n\n")


def diagnostic(
    name,
    start,
    end,
    replacement="\n\n    ",
    rule="statement_spacing_bindings",
    package=None,
):
    primary = {
        "file_name": name,
        "byte_start": start,
        "byte_end": end,
        "is_primary": True,
        "line_start": 2,
    }
    span = dict(
        primary,
        suggested_replacement=replacement,
        suggestion_applicability="MachineApplicable",
        is_primary=False,
    )
    return {
        "reason": "compiler-message",
        "package_id": package,
        "message": {
            "code": {"code": rule},
            "level": "warning",
            "message": "spacing",
            "spans": [primary],
            "children": [{"spans": [span]}],
        },
    }


class WorkspaceTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="statement-spacing-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        (self.root / "src").mkdir()
        self.source = self.root / "src/lib.rs"
        self.source.write_bytes(SOURCE)
        (self.root / "Cargo.toml").write_text(
            '[package]\nname="demo"\nversion="0.1.0"\nedition="2024"\n',
            encoding="utf-8",
        )
        (self.root / "Cargo.lock").write_text(
            'version = 4\n\n[[package]]\nname = "demo"\nversion = "0.1.0"\n',
            encoding="utf-8",
        )
        self.start, self.end = SOURCE.index(b";") + 1, SOURCE.index(b"std::")

    def invoke(self, command="fix", scenario="normal", extra=(), diagnostics=None):
        env = os.environ.copy()
        env.update(
            PYTHON=sys.executable,
            STATEMENT_SPACING_CARGO=str(FAKE_CARGO),
            STATEMENT_SPACING_FAKE_SCENARIO=scenario,
            STATEMENT_SPACING_FAKE_ORIGINAL=str(self.root),
        )
        env.pop("STATEMENT_SPACING_FAKE_DIAGNOSTICS", None)
        if diagnostics is not None:
            env["STATEMENT_SPACING_FAKE_DIAGNOSTICS"] = diagnostics
        env.pop("RUSTFMT", None)
        process = subprocess.run(
            [
                str(BINARY),
                command,
                "--manifest-path",
                str(self.root / "Cargo.toml"),
                "--format-toolchain",
                "1.96.0",
                "--json",
                *extra,
            ],
            cwd=self.root,
            env=env,
            capture_output=True,
            text=True,
            timeout=25,
        )
        try:
            report = json.loads(process.stdout)
        except ValueError:
            self.fail(f"no report: {process.stdout}\n{process.stderr}")
        return process.returncode, report

    def assert_rejected(self, **kwargs):
        before = self.source.read_bytes()
        code, report = self.invoke(**kwargs)
        self.assertEqual(code, 2, report)
        self.assertEqual(self.source.read_bytes(), before)
        return report


class PreviewAndBaselineTests(WorkspaceTest):
    def test_preview_is_verified_and_patch_applies(self):
        code, report = self.invoke(extra=("--dry-run", "--diff"))
        self.assertEqual(code, 1, report)
        self.assertEqual(report["status"], "would-fix")
        self.assertEqual(report["changed_files"], [])
        self.assertEqual(report["proposed_files"], ["src/lib.rs"])
        self.assertTrue(report["verified"]["second_lint_run_clean"])
        self.assertEqual(self.source.read_bytes(), SOURCE)
        applied = subprocess.run(
            ["git", "-c", "core.autocrlf=false", "apply", "--no-index", "-"],
            input=report["diff"].encode("utf-8"),
            cwd=self.root, capture_output=True, check=False,
        )
        self.assertEqual(
            applied.returncode, 0, applied.stderr.decode("utf-8", "replace")
        )
        self.assertEqual(self.source.read_bytes(), FIXED)
        code, report = self.invoke(extra=("--dry-run", "--diff"))
        self.assertEqual(code, 0, report)
        self.assertEqual(report["diff"], "")

    def test_preview_rejects_failed_candidate(self):
        self.assert_rejected(scenario="fmt-conflict", extra=("--dry-run",))

    def test_preview_format_first_does_not_write(self):
        unformatted = SOURCE.replace(b"fn sample", b"fn  sample")
        self.source.write_bytes(unformatted)
        code, report = self.invoke(extra=("--dry-run", "--format-first", "--diff"))
        self.assertEqual(code, 1, report)
        self.assertEqual(self.source.read_bytes(), unformatted)
        self.assertIn("-pub fn  sample", report["diff"])

    def test_baseline_tracks_context_and_multiplicity(self):
        baseline = self.root / "baseline.json"
        code, report = self.invoke("check", extra=("--write-baseline", str(baseline)))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), SOURCE)
        code, report = self.invoke("check", extra=("--baseline", str(baseline)))
        self.assertEqual(code, 0, report)
        self.assertEqual(report["baseline_matched"], 1)
        self.assertEqual(report["findings"], [])
        prefix = b"// unrelated addition\n"
        self.source.write_bytes(prefix + SOURCE)
        items = [diagnostic("src/lib.rs", self.start + len(prefix), self.end + len(prefix), rule="statement_spacing_bindings")]
        # Capture the double's original wording so only location changes.
        code, original_report = self.invoke("check")
        items[0]["message"]["message"] = original_report["findings"][0]["message"]
        code, report = self.invoke("check", extra=("--baseline", str(baseline)), diagnostics="\n".join(map(json.dumps, items)))
        self.assertEqual(code, 0, report)
        self.assertEqual(report["baseline_matched"], 1)
        self.source.write_bytes(prefix + SOURCE + SOURCE)
        items.append(diagnostic("src/lib.rs", self.start + len(prefix) + len(SOURCE), self.end + len(prefix) + len(SOURCE)))
        items[1]["message"]["message"] = items[0]["message"]["message"]
        code, report = self.invoke("check", extra=("--baseline", str(baseline)), diagnostics="\n".join(map(json.dumps, items)))
        self.assertEqual(code, 1, report)
        self.assertEqual(len(report["findings"]), 1)

    def test_baseline_rejects_different_coverage_and_malformed_counts(self):
        baseline = self.root / "baseline.json"
        code, report = self.invoke("check", extra=("--write-baseline", str(baseline)))
        self.assertEqual(code, 0, report)
        self.assert_rejected(command="check", extra=("--baseline", str(baseline), "--all-features"))
        data = json.loads(baseline.read_text())
        data["findings"] = {"0" * 64: -1}
        baseline.write_text(json.dumps(data))
        self.assert_rejected(command="check", extra=("--baseline", str(baseline)))

    def test_report_cannot_overwrite_baseline_through_path_alias(self):
        baseline = self.root / "baseline.json"
        baseline.write_text("preserve this baseline")
        process = subprocess.run(
            [str(BINARY), "check", "--baseline", "baseline.json", "--report", str(baseline)],
            cwd=self.root, capture_output=True, text=True, check=False,
        )
        self.assertEqual(process.returncode, 2)
        self.assertIn("report and baseline paths must differ", process.stderr)
        self.assertEqual(baseline.read_text(), "preserve this baseline")

    def test_library_compiler_identity_is_required_and_stable(self):
        self.assert_rejected(scenario="missing-compiler")
        self.assert_rejected(scenario="compiler-change")

    def test_source_library_does_not_select_existing_binary(self):
        library = self.root / "rules"
        output = library / "target/release"
        output.mkdir(parents=True)
        (library / "Cargo.toml").write_text('[package]\nname="rules"\nversion="0.1.0"\n')
        (output / "libstatement_spacing@old.so").write_bytes(b"stale")
        code, report = self.invoke("check", extra=("--library-path", str(library)))
        self.assertEqual(code, 1, report)
        self.assertNotIn("--lib-path", json.dumps(report["commands"]))
        self.assertIn("--path", json.dumps(report["commands"]))

    def test_explicit_binary_is_fingerprinted_even_in_ignored_target(self):
        output = self.root / "target/release"
        output.mkdir(parents=True)
        library = output / "libstatement_spacing@test.so"
        library.write_bytes(b"explicit library")
        code, report = self.invoke(extra=("--library-path", str(library)))
        self.assertEqual(code, 0, report)
        self.assertEqual(report["library_binary"]["sha256"], hashlib.sha256(library.read_bytes()).hexdigest())
        self.assertIn("--lib-path", json.dumps(report["commands"]))


class EditTests(WorkspaceTest):
    def test_duplicate_targets_share_one_finding_and_edit(self):
        items = [
            diagnostic("src/lib.rs", self.start, self.end, package="library"),
            diagnostic("./src/lib.rs", self.start, self.end, package="library-test"),
        ]
        lines = "\n".join(map(json.dumps, items))
        for command, expected_code in (("check", 1), ("fix", 0)):
            with self.subTest(command=command):
                code, report = self.invoke(command, diagnostics=lines)
                self.assertEqual(code, expected_code, report)
                self.assertEqual(len(report["findings"]), 1)
                self.assertEqual(report["rule_counts"], {"statement_spacing_bindings": 1})
                self.assertEqual(report["findings"][0]["byte_start"], self.start)
        self.assertEqual(self.source.read_bytes(), FIXED)

    def test_distinct_boundaries_on_one_line_remain_distinct(self):
        self.source.write_bytes(b"fn sample() { one(); two(); three(); }\n")
        data = self.source.read_bytes()
        items = [
            diagnostic("src/lib.rs", start, start + 1, "\n\n")
            for start in (data.index(b" two()"), data.index(b" three()"))
        ]
        code, report = self.invoke(
            "check", diagnostics="\n".join(map(json.dumps, items * 2))
        )
        self.assertEqual(code, 1, report)
        self.assertEqual(len(report["findings"]), 2)
        self.assertEqual(report["rule_counts"], {"statement_spacing_bindings": 2})
        self.assertNotEqual(
            report["findings"][0]["byte_start"], report["findings"][1]["byte_start"]
        )

    def test_duplicate_findings_do_not_hide_target_conflicts_or_missing_fixes(self):
        for unfixable in (False, True):
            with self.subTest(unfixable=unfixable):
                first = diagnostic("src/lib.rs", self.start, self.end)
                second = diagnostic("src/lib.rs", self.start, self.end, "\n\n\n    ")
                if unfixable:
                    second["message"]["children"] = []
                self.assert_rejected(
                    diagnostics="\n".join(map(json.dumps, [first, second]))
                )

    def test_invalid_edits_preserve_original(self):
        for start, end, replacement in (
            (-1, 2, "\n"),
            (4, 3, "\n"),
            (0, 10000, "\n"),
            (True, 2, "\n"),
            (0, 3, " "),
            (self.start, self.end, "unsafe {}"),
        ):
            with self.subTest(start=start, end=end, replacement=replacement):
                self.assert_rejected(
                    diagnostics=json.dumps(
                        diagnostic("src/lib.rs", start, end, replacement)
                    )
                )

    def test_overlapping_and_same_position_edits_rejected(self):
        for start in (self.start, self.start + 1):
            with self.subTest(start=start):
                items = [
                    diagnostic("src/lib.rs", self.start, self.end),
                    diagnostic("src/lib.rs", start, start, "\n"),
                ]
                self.assert_rejected(diagnostics="\n".join(map(json.dumps, items)))

    def test_other_lints_and_banners_ignored(self):
        lines = 'Compiling\n[]\nnull\n{"reason":"build-finished"}\n'
        lines += json.dumps(
            diagnostic("src/lib.rs", self.start, self.end, rule="unused_variables")
        )
        code, report = self.invoke("check", diagnostics=lines)
        self.assertEqual(code, 0, report)
        self.assertEqual(report["findings"], [])
        self.assertEqual(self.source.read_bytes(), SOURCE)

    def test_unfixable_alternatives_are_not_guessed(self):
        for ambiguous in (False, True):
            with self.subTest(ambiguous=ambiguous):
                item = diagnostic("src/lib.rs", self.start, self.end)
                if ambiguous:
                    item["message"]["children"] *= 2
                else:
                    item["message"]["children"][0]["spans"][0][
                        "suggestion_applicability"
                    ] = "MaybeIncorrect"
                self.assert_rejected(diagnostics=json.dumps(item))

    def test_raw_rustc_diagnostic_supported(self):
        item = diagnostic("src/lib.rs", self.start, self.end)["message"]
        item["$message_type"] = "diagnostic"
        code, report = self.invoke(diagnostics=json.dumps(item))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), FIXED)

    def test_utf8_crlf_byte_offsets(self):
        data = "// Žiga 🦀\r\n".encode() + SOURCE.replace(b"\n", b"\r\n")
        self.source.write_bytes(data)
        start, end = data.index(b";") + 1, data.index(b"std::")
        item = diagnostic("src/lib.rs", start, end, "\r\n\r\n    ")
        code, report = self.invoke(diagnostics=json.dumps(item))
        self.assertEqual(code, 0, report)
        self.assertEqual(
            self.source.read_bytes(), data[:start] + b"\r\n\r\n    " + data[end:]
        )

    def test_non_rust_and_outside_paths_rejected(self):
        with tempfile.NamedTemporaryFile(suffix=".rs") as outside:
            for name in ("Cargo.toml", outside.name):
                with self.subTest(name=name):
                    self.assert_rejected(
                        diagnostics=json.dumps(diagnostic(name, 0, 0, "\n"))
                    )

    def test_workspace_relative_member_paths(self):
        member = self.root / "member"
        (member / "src").mkdir(parents=True)
        (member / "src/lib.rs").write_bytes(SOURCE)
        (member / "Cargo.toml").write_text(
            '[package]\nname="member"\nversion="0.1.0"\n'
        )
        item = diagnostic("member/src/lib.rs", self.start, self.end, package="member-id")
        code, report = self.invoke(
            scenario="multi-package", diagnostics=json.dumps(item)
        )
        self.assertEqual(code, 0, report)
        self.assertEqual(
            [finding["file"] for finding in report["findings"]], ["member/src/lib.rs"]
        )
        self.assertEqual((member / "src/lib.rs").read_bytes(), FIXED)
        self.assertEqual(self.source.read_bytes(), SOURCE)


class SnapshotTests(WorkspaceTest):
    def test_nested_repository_ignores_match_in_snapshot_without_git_metadata(self):
        for marker_is_directory in (False, True):
            with self.subTest(marker_is_directory=marker_is_directory):
                self.source.write_bytes(SOURCE)
                vendor = self.root / f"vendor/dependency-{marker_is_directory}"
                (vendor / "zig-out").mkdir(parents=True)
                (vendor / ".zig-cache").mkdir()
                (vendor / "src/generated").mkdir(parents=True)
                if marker_is_directory:
                    (vendor / ".git").mkdir()
                else:
                    (vendor / ".git").write_text(
                        "gitdir: ../../.git/modules/dependency\n"
                    )
                (vendor / ".gitignore").write_text("/zig-out/\n/.zig-cache/\n")
                (vendor / "src/.gitignore").write_text("/generated/\n")
                (vendor / "zig-out/Cargo.toml").write_text("not a valid manifest")
                (vendor / ".zig-cache/output").write_text("cache artifact")
                (vendor / "src/generated/output").write_text("generated artifact")
                (vendor / "src/source.txt").write_text("source dependency")
                code, report = self.invoke()
                self.assertEqual(code, 0, report)
                self.assertEqual(self.source.read_bytes(), FIXED)
                self.assertEqual(
                    (vendor / "zig-out/Cargo.toml").read_text(), "not a valid manifest"
                )
                self.assertEqual(
                    (vendor / "src/source.txt").read_text(), "source dependency"
                )

    def test_build_and_state_dirs_ignored(self):
        for name in ("target", ".git", ".statement-spacing", "__pycache__"):
            (self.root / name).mkdir()
            try:
                (self.root / name / "ignored").symlink_to(self.source)
            except OSError:
                (self.root / name / "ignored").write_bytes(self.source.read_bytes())
        code, report = self.invoke()
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), FIXED)

    def test_original_added_file_and_mode_changes_abort(self):
        scenarios = ["original-added-file"]
        if sys.platform != "win32":
            scenarios.append("original-mode-change")
        for scenario in scenarios:
            with self.subTest(scenario=scenario):
                self.source.chmod(0o644)
                self.assert_rejected(scenario=scenario)
                if scenario == "original-added-file":
                    self.assertEqual((self.root / "new").read_text(), "editor addition")
                else:
                    self.assertEqual(self.source.stat().st_mode & 0o777, 0o600)

    def test_size_limit(self):
        (self.root / "large").write_bytes(b"x" * (1024 * 1024))
        self.assert_rejected(extra=("--max-snapshot-mib", "1"))

    def test_symlinks_rejected(self):
        for target in (self.source, self.root / "src"):
            with self.subTest(target=target):
                alias = self.root / "alias"
                try:
                    alias.symlink_to(target)
                except OSError:
                    self.skipTest("symlinks require elevated privileges on Windows")
                try:
                    self.assert_rejected()
                finally:
                    if alias.is_symlink() or alias.exists():
                        alias.unlink()

    def test_absolute_manifest_path_rejected(self):
        (self.root / "Cargo.toml").write_text('[package]\nbuild="/tmp/build.rs"\n')
        self.assert_rejected()

    def test_concurrent_lock_rejected(self):
        state = self.root / ".statement-spacing"
        state.mkdir()
        lock_path = state / "lock"
        if sys.platform == "win32":
            with lock_path.open("w") as lock:
                try:
                    msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
                except OSError:
                    self.skipTest("msvcrt locking unavailable")
                self.assert_rejected()
        elif fcntl is not None:
            with lock_path.open("w") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                self.assert_rejected()
        else:
            self.skipTest("file locking unavailable on this platform")


class TransactionTests(WorkspaceTest):
    def make_journal(self, path="src/lib.rs", second=False):
        directory = self.root / ".statement-spacing/transactions/test"
        directory.mkdir(parents=True)
        (directory / "0.original").write_bytes(SOURCE)
        entries = [
            {
                "path": path,
                "backup": "0.original",
                "before": hashlib.sha256(SOURCE).hexdigest(),
                "after": hashlib.sha256(FIXED).hexdigest(),
                "mode": 0o644,
            }
        ]
        if second:
            (self.root / "src/other.rs").write_bytes(SOURCE)
            entries.append(dict(entries[0], path="src/other.rs"))
        (directory / "journal.json").write_text(
            json.dumps({"schema": 1, "phase": "prepared", "files": entries})
        )
        return directory

    def test_commit_preserves_mode_and_cleans_journal(self):
        self.source.chmod(0o640)
        code, report = self.invoke()
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), FIXED)
        if sys.platform != "win32":
            self.assertEqual(self.source.stat().st_mode & 0o777, 0o640)
        self.assertEqual(
            list((self.root / ".statement-spacing/transactions").iterdir()), []
        )

    def test_rollback_preserves_newer_editor_content(self):
        code, report = self.invoke(scenario="rollback-editor")
        self.assertEqual(code, 2, report)
        self.assertEqual(
            self.source.read_bytes(), b"// user edit after candidate commit\n"
        )
        backups = list(
            (self.root / ".statement-spacing/transactions").glob("*/*.original")
        )
        self.assertEqual([path.read_bytes() for path in backups], [SOURCE])

    def test_hardlinked_source_rejected(self):
        os.link(self.source, self.root / "hardlink.rs")
        self.assert_rejected()
        self.assertEqual((self.root / "hardlink.rs").read_bytes(), SOURCE)

    def test_recover_partial_multi_file_commit(self):
        directory = self.make_journal(second=True)
        self.source.write_bytes(FIXED)
        code, report = self.invoke("recover")
        self.assertEqual(code, 0, report)
        self.assertEqual(report["recovered_transactions"], ["test"])
        self.assertEqual(self.source.read_bytes(), SOURCE)
        self.assertEqual((self.root / "src/other.rs").read_bytes(), SOURCE)
        self.assertFalse(directory.exists())

    def test_recover_preserves_later_change(self):
        directory = self.make_journal()
        self.source.write_bytes(b"newer")
        self.assert_rejected(command="recover")
        self.assertEqual((directory / "0.original").read_bytes(), SOURCE)

    def test_corrupt_backup_rejected(self):
        directory = self.make_journal()
        (directory / "0.original").write_bytes(b"corrupt")
        self.source.write_bytes(FIXED)
        self.assert_rejected(command="recover")
        self.assertTrue(directory.exists())

    def test_unsafe_recovery_names_rejected(self):
        directory = self.make_journal()
        journal = json.loads((directory / "journal.json").read_text())
        for name in ("", ".", "..", "../x", "/tmp/x", "x/../y"):
            with self.subTest(name=name):
                journal["files"][0]["path"] = name
                (directory / "journal.json").write_text(json.dumps(journal))
                self.assert_rejected(command="recover")


class SubprocessWorkflowTests(WorkspaceTest):
    def test_fix_two_lints_and_formatter_checks(self):
        code, report = self.invoke()
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), FIXED)
        self.assertEqual(report["changed_files"], ["src/lib.rs"])
        self.assertTrue(all(report["verified"].values()))
        lints = [c for c in report["commands"] if "dylint" in c["argv"]]
        self.assertEqual(len(lints), 2)
        fmts = [
            c["argv"]
            for c in report["commands"]
            if "fmt" in c["argv"] and "--version" not in c["argv"]
        ]
        self.assertTrue(all("--check" in command for command in fmts))

    def test_check_preserves_sources(self):
        code, report = self.invoke("check")
        self.assertEqual(code, 1, report)
        self.assertEqual(self.source.read_bytes(), SOURCE)

    def test_clean_check(self):
        self.source.write_bytes(FIXED)
        code, report = self.invoke("check")
        self.assertEqual(code, 0, report)

    def test_clean_fix_noop(self):
        self.source.write_bytes(FIXED)
        code, report = self.invoke()
        self.assertEqual(code, 0, report)
        self.assertEqual(report["changed_files"], [])

    def test_repeated_fix_noop(self):
        self.assertEqual(self.invoke()[0], 0)
        code, report = self.invoke()
        self.assertEqual(code, 0, report)
        self.assertEqual(report["changed_files"], [])

    def test_candidate_formatter_conflict(self):
        code, report = self.invoke(scenario="fmt-conflict")
        self.assertEqual(code, 2, report)
        self.assertEqual(self.source.read_bytes(), SOURCE)

    def test_original_formatter_failure_rolls_back(self):
        code, report = self.invoke(scenario="original-fmt-conflict")
        self.assertEqual(code, 2, report)
        self.assertEqual(self.source.read_bytes(), SOURCE)
        self.assertFalse(
            list((self.root / ".statement-spacing/transactions").iterdir())
        )

    def test_formatter_identity_change_rolls_back(self):
        code, report = self.invoke(scenario="identity-change")
        self.assertEqual(code, 2, report)
        self.assertEqual(self.source.read_bytes(), SOURCE)

    def test_fail_closed_scenarios(self):
        for scenario in (
            "missing-probe",
            "stale-probe",
            "wrong-version",
            "compiler-error",
            "token-edit",
            "conflicting",
            "no-fix",
            "not-idempotent",
            "clone-write",
            "coverage-change",
            "token-drift",
            "missing-token-hash",
        ):
            with self.subTest(scenario=scenario):
                code, report = self.invoke(scenario=scenario)
                self.assertEqual(code, 2, report)
                self.assertEqual(self.source.read_bytes(), SOURCE)

    def test_denied_findings_fixable(self):
        code, report = self.invoke(scenario="denied")
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), FIXED)

    def test_duplicate_target_fixes_deduplicated(self):
        code, report = self.invoke(scenario="duplicate")
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), FIXED)

    def test_concurrent_original_edit_preserved(self):
        code, report = self.invoke(scenario="concurrent-original")
        self.assertEqual(code, 2, report)
        self.assertEqual(self.source.read_bytes(), b"// editor saved new content\n")

    def test_unformatted_baseline_rejected(self):
        unformatted = SOURCE.replace(b"fn sample", b"fn  sample")
        self.source.write_bytes(unformatted)
        code, report = self.invoke()
        self.assertEqual(code, 2, report)
        self.assertEqual(self.source.read_bytes(), unformatted)

    def test_format_first_writes_only_replica(self):
        self.source.write_bytes(SOURCE.replace(b"fn sample", b"fn  sample"))
        code, report = self.invoke(extra=("--format-first",))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.source.read_bytes(), FIXED)
        writing = [
            c
            for c in report["commands"]
            if "fmt" in c["argv"]
            and "--version" not in c["argv"]
            and "--check" not in c["argv"]
        ]
        self.assertEqual(len(writing), 1)
        self.assertNotEqual(writing[0]["cwd"], str(self.root))

    def test_missing_lockfile_rejected(self):
        (self.root / "Cargo.lock").unlink()
        code, report = self.invoke()
        self.assertEqual(code, 2, report)

    def test_features_forwarded(self):
        code, report = self.invoke(
            extra=(
                "--features",
                "one",
                "--features",
                "two",
                "--no-default-features",
                "--offline",
            )
        )
        self.assertEqual(code, 0, report)
        lints = [c["argv"] for c in report["commands"] if "dylint" in c["argv"]]
        self.assertTrue(
            all("one,two" in args and "--no-default-features" in args for args in lints)
        )

    def test_recover_empty(self):
        code, report = self.invoke(command="recover")
        self.assertEqual(code, 0, report)
        self.assertEqual(report["recovered_transactions"], [])

    def test_toolchain_detected_from_rust_toolchain_toml(self):
        (self.root / "rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.97.1"\n', encoding="utf-8"
        )
        env = os.environ.copy()
        env.update(
            PYTHON=sys.executable,
            STATEMENT_SPACING_CARGO=str(FAKE_CARGO),
            STATEMENT_SPACING_FAKE_SCENARIO="normal",
            STATEMENT_SPACING_FAKE_ORIGINAL=str(self.root),
        )
        env.pop("STATEMENT_SPACING_FAKE_DIAGNOSTICS", None)
        env.pop("RUSTFMT", None)
        process = subprocess.run(
            [
                str(BINARY),
                "fix",
                "--manifest-path",
                str(self.root / "Cargo.toml"),
                "--json",
            ],
            cwd=self.root,
            env=env,
            capture_output=True,
            text=True,
            timeout=25,
        )
        report = json.loads(process.stdout)
        self.assertEqual(process.returncode, 0, report)
        self.assertEqual(report["formatter"]["toolchain"], "1.97.1")
        fmts = [
            c["argv"]
            for c in report["commands"]
            if "fmt" in c["argv"] and "--manifest-path" in c["argv"]
        ]
        self.assertTrue(all(c[1] == "+1.97.1" for c in fmts))


if __name__ == "__main__":
    unittest.main()
