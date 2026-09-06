#!/usr/bin/env python3
"""Dependency-free artifact integrity checks; NOT a Rust parser/compiler test."""

import ast
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
IGNORED = {"target", ".git", ".statement-spacing", "__pycache__", "validation-output"}


def files(suffix):
    return sorted(
        path
        for path in ROOT.rglob("*" + suffix)
        if not set(path.relative_to(ROOT).parts) & IGNORED
    )


def main():
    report = {"kind": "static-artifact-integrity", "rust_compilation": "NOT_RUN"}
    toml_files = files(".toml")
    for path in toml_files:
        value = tomllib.loads(path.read_text(encoding="utf-8"))
        if path.name == "Cargo.toml":
            for table in ("dependencies", "dev-dependencies", "build-dependencies"):
                dependencies = value.get(table, {})
                for spec in dependencies.values():
                    if isinstance(spec, dict) and "path" in spec:
                        target = (path.parent / spec["path"] / "Cargo.toml").resolve()
                        assert target.is_file(), f"missing local dependency: {target}"
            for target in value.get("bin", []):
                assert (path.parent / target["path"]).is_file(), (
                    f"missing binary target in {path}"
                )
    report["toml_documents"] = len(toml_files)
    lockfiles = files(".lock")
    for path in lockfiles:
        if path.name == "Cargo.lock":
            tomllib.loads(path.read_text(encoding="utf-8"))
    report["included_lockfiles"] = len(lockfiles)
    python_files = files(".py")
    for path in python_files:
        ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    report["python_ast_parsed"] = len(python_files)
    # Executable-stub policy is checked by `cargo run -p source-policy`.
    # A raw-text scan would also reject Rust examples inside literals/comments.
    report["rust_source_files"] = len(files(".rs"))
    root_manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    for member in root_manifest["workspace"]["members"]:
        assert (ROOT / member / "Cargo.toml").is_file(), (
            f"missing workspace member: {member}"
        )
    lint_manifest = tomllib.loads((ROOT / "lint/Cargo.toml").read_text())
    assert lint_manifest["lib"]["name"] == "statement_spacing"
    assert "cdylib" in lint_manifest["lib"]["crate-type"]
    assert (
        root_manifest["workspace"]["package"]["version"]
        == lint_manifest["package"]["version"]
    )
    expected_rules = {
        "bindings",
        "expressions",
        "control_flow",
        "after_block",
        "exit",
        "result_check",
        "item_spacing",
        "layout",
    }
    # This is only a textual declaration inventory, not compiler registration.
    # scripts/validate_registration.py checks the loaded lints and their group.
    declarations = set(
        re.findall(
            r"declare_lint\s*!\s*\(\s*pub\s+STATEMENT_SPACING_(\w+)",
            (ROOT / "lint/src/lib.rs").read_text(),
        )
    )
    assert {name.lower() for name in declarations} == expected_rules
    report["declared_lint_ids"] = len(declarations)
    for document in ("README.md", "LICENSE-MIT", "LICENSE-APACHE"):
        assert (ROOT / document).is_file(), (
            f"missing required documentation: {document}"
        )
    fixture = (ROOT / "tests/workspaces/basic/src/lib.rs").read_bytes()
    fixed = (ROOT / "tests/fixtures/basic.fixed.rs").read_bytes()
    assert fixture != fixed
    assert re.sub(rb"\s+", b"", fixture) == re.sub(rb"\s+", b"", fixed)
    report["golden_fixture_whitespace_only_difference"] = True
    report["status"] = "passed"
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
