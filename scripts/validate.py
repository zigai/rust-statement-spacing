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
                    ["cargo", "+nightly-2025-09-18", "build", "--release", "--locked"],
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
                    assert any(
                        f["rule"] == "statement_spacing_bindings"
                        for f in first_report["findings"]
                    )
                    assert source.read_bytes() == before, "check modified source"
                    fixed = run(base + ["fix", *arguments], fixture)
                    assert json.loads(fixed.stdout)["verified"]["second_lint_run_clean"]
                    assert source.read_bytes() == expected, (
                        "fixed output differs from explicit golden file"
                    )
                    # Hard requirement: use --check; do not write-format the candidate.
                    run(["cargo", "+1.96.0", "fmt", "--all", "--", "--check"], fixture)
                    again = run(base + ["fix", *arguments], fixture)
                    assert not json.loads(again.stdout)["changed_files"], (
                        "fix not idempotent"
                    )
                    run(base + ["check", *arguments], fixture)
                    run(["cargo", "+1.96.0", "check", "--locked"], fixture)

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
                    prebuilt_so = next(
                        (
                            p
                            for p in (
                                *(ROOT / "lint/target/release").glob("libstatement_spacing@*"),
                                *(ROOT / "lint/target/dylint/libraries").glob(
                                    "*/release/libstatement_spacing@*"
                                ),
                            )
                            if p.is_file() and p.suffix in (".so", ".dylib", ".dll")
                        ),
                        None,
                    )
                    if prebuilt_so:
                        run(
                            [
                                "cargo",
                                "dylint",
                                "--lib-path",
                                prebuilt_so,
                                "--fix",
                                "--",
                                "--all-targets",
                                "--locked",
                            ],
                            fixture,
                        )
                    else:
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
