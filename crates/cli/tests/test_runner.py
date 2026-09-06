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
                "1.90.0",
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


class EditTests(WorkspaceTest):
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

    def test_package_relative_and_unscoped_paths(self):
        member = self.root / "member"
        (member / "src").mkdir(parents=True)
        (member / "src/lib.rs").write_bytes(SOURCE)
        (member / "Cargo.toml").write_text(
            '[package]\nname="member"\nversion="0.1.0"\n'
        )
        item = diagnostic("src/lib.rs", self.start, self.end, package="member-id")
        code, report = self.invoke(
            "check", scenario="multi-package", diagnostics=json.dumps(item)
        )
        self.assertEqual(code, 1, report)
        self.assertEqual(
            [finding["file"] for finding in report["findings"]], ["member/src/lib.rs"]
        )
        self.assertEqual((member / "src/lib.rs").read_bytes(), SOURCE)
        item["package_id"] = None
        self.assert_rejected(
            command="check", scenario="multi-package", diagnostics=json.dumps(item)
        )


class SnapshotTests(WorkspaceTest):
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


if __name__ == "__main__":
    unittest.main()
