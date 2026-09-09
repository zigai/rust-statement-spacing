#!/usr/bin/env python3
"""Real Rust/Dylint validation. No test doubles and no post-fix formatting repair.

Requires scripts/bootstrap.sh to have completed. All reports identify the actual
commands and exit statuses. Fails rather than silently skipping missing tools.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--portable-only",
        action="store_true",
        help="Rust unit/parser tests, without rustc-private build",
    )
    parser.add_argument(
        "--report", type=Path, default=ROOT / "validation-output/real-validation.json"
    )
    options = parser.parse_args()
    report = {
        "schema": 1,
        "kind": "real-rust-validation",
        "started_unix": time.time(),
        "commands": [],
    }

    def run(args, cwd=ROOT, expected=(0,), env=None):
        print("+", " ".join(map(str, args)), flush=True)
        process = subprocess.run(
            list(map(str, args)), cwd=cwd, env=env, text=True, capture_output=True
        )
        report["commands"].append(
            {
                "argv": list(map(str, args)),
                "cwd": str(cwd),
                "exit": process.returncode,
                "stdout": process.stdout,
                "stderr": process.stderr,
            }
        )
        if process.returncode not in expected:
            print(process.stdout, end="")
            print(process.stderr, end="", file=sys.stderr)
            raise RuntimeError(f"command exited {process.returncode}: {args}")
        return process

    try:
        if not shutil.which("cargo") or not shutil.which("rustup"):
            raise RuntimeError(
                "Rust is unavailable: install rustup, then run scripts/bootstrap.sh"
            )
        if not (ROOT / "Cargo.lock").exists():
            raise RuntimeError(
                "Resolve dependency locks with scripts/bootstrap.sh first"
            )
        run(["cargo", "+1.96.0", "build", "-p", "cargo-statement-spacing", "--locked"])
        run(["cargo", "+1.96.0", "run", "-p", "source-policy", "--locked"])
        run(
            [
                sys.executable,
                "-m",
                "unittest",
                "discover",
                "-s",
                "crates/cli/tests",
                "-v",
            ]
        )
        run(["cargo", "+1.96.0", "test", "--workspace", "--locked"])
        with tempfile.TemporaryDirectory(prefix="statement-spacing-installed-") as name:
            install_root = Path(name)
            run(
                [
                    "cargo",
                    "+1.96.0",
                    "install",
                    "--path",
                    ROOT / "crates/cli",
                    "--locked",
                    "--root",
                    install_root,
                ]
            )
            installed = install_root / "bin/cargo-statement-spacing"
            version = run([installed, "--version"], install_root)
            assert version.stdout.startswith("statement-spacing "), (
                "installed consumer omitted version"
            )
            if not options.portable_only:
                run(
                    ["cargo", "+nightly-2026-05-28", "build", "--release", "--locked"],
                    ROOT / "lint",
                )
                run([sys.executable, "scripts/validate_registration.py"])
                # Real checks below use an installed consumer outside the source tree.
                with tempfile.TemporaryDirectory(
                    prefix="statement-spacing-real-validation-"
                ) as name:
                    temporary = Path(name)
                    fixture = temporary / "fixture"
                    shutil.copytree(ROOT / "tests/workspaces/basic", fixture)
                    # Nested Git metadata is omitted from replicas, but its
                    # ignore policy must remain identical during every scan.
                    dependency = fixture / "vendor/native-library"
                    (dependency / "zig-out").mkdir(parents=True)
                    (dependency / ".zig-cache").mkdir()
                    (dependency / ".git").write_text("gitdir: ../../.git/modules/native-library\n")
                    (dependency / ".gitignore").write_text("/zig-out/\n/.zig-cache/\n")
                    (dependency / "zig-out/artifact").write_bytes(b"ignored build output")
                    (dependency / ".zig-cache/artifact").write_bytes(b"ignored cache output")
                    (dependency / "source.txt").write_text("dependency source\n")
                    source = fixture / "src/lib.rs"
                    before = source.read_bytes()
                    expected = (ROOT / "tests/fixtures/basic.fixed.rs").read_bytes()
                    base = [installed]
                    arguments = [
                        "--manifest-path",
                        fixture / "Cargo.toml",
                        "--library-path",
                        ROOT / "lint",
                        "--format-toolchain",
                        "1.96.0",
                        "--json",
                    ]
                    run(["cargo", "+1.96.0", "fmt", "--all", "--", "--check"], fixture)
                    first = run(base + ["check", *arguments], fixture, expected=(1,))
                    first_report = json.loads(first.stdout)
                    assert len(first_report["findings"]) == 1, (
                        "library and test compilations duplicated a finding"
                    )
                    assert first_report["rule_counts"] == {"statement_spacing_bindings": 1}
                    assert any(
                        f["rule"] == "statement_spacing_bindings"
                        for f in first_report["findings"]
                    )
                    assert source.read_bytes() == before, "check modified source"
                    preview = run(base + ["fix", "--dry-run", "--diff", *arguments], fixture, expected=(1,))
                    preview_report = json.loads(preview.stdout)
                    assert preview_report["verified"]["second_lint_run_clean"]
                    assert preview_report["changed_files"] == []
                    assert preview_report["diff"].startswith("--- ")
                    assert source.read_bytes() == before, "preview modified source"
                    baseline = temporary / "baseline.json"
                    run(base + ["check", "--write-baseline", baseline, *arguments], fixture)
                    baselined = run(base + ["check", "--baseline", baseline, *arguments], fixture)
                    assert json.loads(baselined.stdout)["baseline_matched"] == 1
                    assert source.read_bytes() == before, "baseline modified source"
                    fixed = run(base + ["fix", *arguments], fixture)
                    assert json.loads(fixed.stdout)["verified"]["second_lint_run_clean"]
                    assert source.read_bytes() == expected, (
                        "fixed output differs from explicit golden file"
                    )
                    assert (dependency / "zig-out/artifact").read_bytes() == b"ignored build output"
                    assert (dependency / "source.txt").read_text() == "dependency source\n"
                    # Hard requirement: use --check; do not write-format the candidate.
                    run(["cargo", "+1.96.0", "fmt", "--all", "--", "--check"], fixture)
                    again = run(base + ["fix", *arguments], fixture)
                    assert not json.loads(again.stdout)["changed_files"], (
                        "fix not idempotent"
                    )
                    run(base + ["check", *arguments], fixture)
                    run(["cargo", "+1.96.0", "check", "--locked"], fixture)

                    # Real compiler regressions for cohesive operation groups.
                    grouped = (ROOT / "tests/fixtures/grouping.rs").read_bytes()
                    source.write_bytes(grouped)
                    run(base + ["check", *arguments], fixture)
                    grouped_fix = run(base + ["fix", *arguments], fixture)
                    assert not json.loads(grouped_fix.stdout)["changed_files"]
                    assert source.read_bytes() == grouped, (
                        "fix fragmented an intentionally cohesive operation group"
                    )

                    # Relatedness must not leak through a shared nested callee,
                    # or turn an immutable receiver into a mutation.
                    compact = grouped
                    for boundary in (
                        b"alpha(left);\n\n    beta(right);",
                        b"alpha(identity(left));\n\n    beta(identity(right));",
                        b"values.len();\n\n    counter += 1;",
                        b"alpha(1);\n    }\n\n    if right",
                        b"}\n\n    for value in third",
                        b"}\n\n    'scan: for value in second",
                        b"}\n\n    while remaining > 0",
                        b"black_box(&raw const value);\n\n    return value;",
                        b"let _fill_later = || &raw mut value;\n\n    return value;",
                        b"let registered = generated.len();\n\n    let mut checked_in",
                    ):
                        assert boundary in compact
                        compact = compact.replace(boundary, boundary.replace(b"\n\n", b"\n"))
                    source.write_bytes(compact)
                    run(base + ["fix", *arguments], fixture)
                    assert source.read_bytes() == grouped, (
                        "fix failed to separate unrelated operations or control blocks"
                    )
                    (fixture / "dylint.toml").write_text(
                        '[statement_spacing.grouping]\nexpressions="strict"\n'
                    )
                    run(base + ["check", *arguments], fixture, expected=(1,))
                    (fixture / "dylint.toml").write_text("[statement_spacing]\n")

                    # Audit regressions use the visual normalization preferences.
                    audit = (ROOT / "tests/fixtures/audit.rs").read_bytes()
                    source.write_bytes(audit)
                    (fixture / "dylint.toml").write_text(
                        '[statement_spacing.grouping]\n'
                        'bindings="multiline"\nexpressions="multiline"\n'
                        'self_fields="distinct"\njoin_related=true\n'
                        '[statement_spacing.control_flow]\n'
                        'guard_chain="contextual"\nrelated_continuation=false\n'
                        '[statement_spacing.exits]\ntail="visual"\n'
                    )
                    run(base + ["check", *arguments], fixture)
                    for candidate in (
                        audit.replace(b'"#;\n\nconst', b'"#;\nconst')
                        .replace(b"\n\n    let mut masks", b"\n    let mut masks")
                        .replace(b"\n\n    fs::remove_file", b"\n    fs::remove_file"),
                        audit.replace(b"    let mut health", b"\n    let mut health")
                        .replace(b"    let mut last_dash", b"\n    let mut last_dash")
                        .replace(b"    for clip", b"\n    for clip")
                        .replace(b"    if !status", b"\n    if !status")
                        .replace(b"        if names", b"\n        if names")
                        .replace(b"        if (product", b"\n        if (product")
                        .replace(b"    let hex", b"\n    let hex")
                        .replace(b"    let value = entry", b"\n    let value = entry")
                        .replace(b"    validate_name(name)", b"\n    validate_name(name)")
                        .replace(b"    if sim.golden", b"\n    if sim.golden")
                        .replace(b"    dirty |= ui", b"\n    dirty |= ui")
                        .replace(b"    assert!(verify", b"\n    assert!(verify")
                        .replace(b"    b.push", b"\n    b.push")
                        .replace(b"    let mut enabled", b"\n    let mut enabled")
                        .replace(b"    assert!(enabled", b"\n    assert!(enabled")
                        .replace(b"    assert!(!enabled", b"\n    assert!(!enabled"),
                    ):
                        source.write_bytes(candidate)
                        run(base + ["fix", *arguments], fixture)
                        assert source.read_bytes() == audit, "audit grouping regression"
                        run(base + ["check", *arguments], fixture)
                        run(["cargo", "+1.96.0", "fmt", "--all", "--", "--check"], fixture)
                    source.write_bytes(grouped)
                    (fixture / "dylint.toml").write_text("[statement_spacing]\n")

                    # Suppression and expectation operate at actual compiler nodes.
                    for attribute in ("allow", "expect"):
                        source.write_text(
                            "#![deny(unfulfilled_lint_expectations)]\n"
                            f'#[cfg_attr(dylint_lib = "statement_spacing", {attribute}(statement_spacing_bindings))]\n'
                            + before.decode(),
                            encoding="utf-8",
                        )
                        run(["cargo", "+1.96.0", "fmt", "--all"], fixture)  # baseline only
                        run(base + ["check", *arguments], fixture)

                    # Fail closed on invalid user configuration.
                    (fixture / "dylint.toml").write_text(
                        '[statement_spacing.grouping]\nexpressions="typo"\n'
                    )
                    run(base + ["check", *arguments], fixture, expected=(2,))

                    # Native Dylint --fix path, independently of the verified wrapper.
                    (fixture / "dylint.toml").write_text("[statement_spacing]\n")
                    source.write_bytes(before)
                    # The synthetic submodule marker was needed for verified
                    # snapshot checks; it is not a real Git repository.
                    (dependency / ".git").unlink()
                    run(["git", "init", "-q"], fixture)
                    run(["git", "add", "."], fixture)
                    run(
                        [
                            "git",
                            "-c",
                            "user.name=Statement spacing fixture",
                            "-c",
                            "user.email=fixture@example.invalid",
                            "commit",
                            "-qm",
                            "fixture baseline",
                        ],
                        fixture,
                    )
                    env = os.environ.copy()
                    env["CARGO_TARGET_DIR"] = str(temporary / "native-target")
                    run(
                        [
                            "cargo",
                            "dylint",
                            "--path",
                            ROOT / "lint",
                            "--fix",
                            "--",
                            "--all-targets",
                            "--locked",
                        ],
                        fixture,
                        env=env,
                    )
                    assert source.read_bytes() == expected, (
                        "native Dylint fix differs from golden output"
                    )
                    run(["cargo", "+1.96.0", "fmt", "--all", "--", "--check"], fixture)
                    report["golden_sha256"] = hashlib.sha256(expected).hexdigest()
        report["status"] = "passed"
        return 0
    except (OSError, RuntimeError, AssertionError, ValueError) as error:
        report["status"] = "failed"
        report["error"] = str(error)
        print(f"validation failed: {error}", file=sys.stderr)
        return 1
    finally:
        options.report.parent.mkdir(parents=True, exist_ok=True)
        options.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
