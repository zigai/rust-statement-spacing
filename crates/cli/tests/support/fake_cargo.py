#!/usr/bin/env -S python3 -S
"""Explicit external-process test double, NOT a Rust compiler or formatter.

Exercises the real transaction driver using deterministic Cargo/Dylint-shaped
responses. No passing test using this file establishes Rust compilation.
"""

import json
import hashlib
import re
import os
from pathlib import Path
import sys

args = sys.argv[1:]
if args and args[0].startswith("+"):
    args.pop(0)
scenario = os.environ.get("STATEMENT_SPACING_FAKE_SCENARIO", "normal")
root = Path.cwd()
if "--manifest-path" in args:
    root = Path(args[args.index("--manifest-path") + 1]).resolve().parent
source = root / "src/lib.rs"
original = Path(os.environ.get("STATEMENT_SPACING_FAKE_ORIGINAL", str(root))).resolve()
package_id = "path+file:///fixture#demo@0.1.0"


def emit(value):
    print(json.dumps(value))


if args == ["--version"]:
    print("cargo 1.90.0 (test double)")
elif args[:2] == ["fmt", "--version"]:
    changed = source.exists() and b";\n\n" in source.read_bytes()
    if scenario == "identity-change" and root == original and changed:
        print("rustfmt CHANGED (test double)")
    else:
        print("rustfmt 1.8.0 (test double)")
elif args[0] == "locate-project":
    emit({"root": str(root / "Cargo.toml")})
elif args[0] == "metadata":
    packages = [
        {
            "id": package_id,
            "manifest_path": str(root / "Cargo.toml"),
            "targets": [{"src_path": str(source)}],
            "dependencies": [],
        }
    ]
    if scenario == "multi-package":
        packages.append(
            {
                "id": "member-id",
                "manifest_path": str(root / "member/Cargo.toml"),
                "targets": [{"src_path": str(root / "member/src/lib.rs")}],
                "dependencies": [],
            }
        )
    emit(
        {
            "workspace_root": str(root),
            "workspace_members": [p["id"] for p in packages],
            "packages": packages,
        }
    )
elif args[0] == "fmt":
    data = source.read_bytes()
    if "--check" not in args:
        source.write_bytes(data.replace(b"fn  sample", b"fn sample"))
    else:
        conflict = b"fn  sample" in data
        conflict |= scenario == "fmt-conflict" and root != original and b";\n\n" in data
        conflict |= (
            scenario == "original-fmt-conflict"
            and root == original
            and b";\n\n" in data
        )
        if scenario == "rollback-editor" and root == original and b";\n\n" in data:
            source.write_bytes(b"// user edit after candidate commit\n")
            conflict = True
        if conflict:
            print("Diff: this candidate is not a formatter fixed point (test double)")
            sys.exit(1)
elif args[0] == "dylint":
    label = Path(os.environ["STATEMENT_SPACING_PROBE_DIR"]).name.split("-")[0]
    if scenario == "clone-write":
        (root / "Cargo.toml").write_text(
            (root / "Cargo.toml").read_text() + "\n# build wrote this\n"
        )
    if scenario == "concurrent-original" and label == "initial":
        (original / "src/lib.rs").write_bytes(b"// editor saved new content\n")
    if scenario == "original-added-file" and label == "initial":
        (original / "new").write_text("editor addition")
    if scenario == "original-mode-change" and label == "initial":
        (original / "src/lib.rs").chmod(0o600)
    if scenario != "missing-probe":
        checked_sources = [source]
        if scenario == "multi-package":
            checked_sources.append(root / "member/src/lib.rs")
        handshake = {
            "schema": 1,
            "library": "statement_spacing",
            "version": "0.1.0",
            "run_id": os.environ["STATEMENT_SPACING_RUN_ID"],
            "files": list(map(str, checked_sources)),
            "skipped_boundaries": 0,
            # The double models its one simple fixture, not a Rust token parser.
            # Production fingerprints come exclusively from the Rust syntax adapter.
            "token_hashes": {
                str(path): hashlib.sha256(
                    re.sub(rb"\s+", b"", path.read_bytes())
                ).hexdigest()
                for path in checked_sources
            },
        }
        if scenario == "stale-probe":
            handshake["run_id"] = "stale"
        if scenario == "missing-token-hash":
            handshake.pop("token_hashes")
        if scenario == "token-drift" and label == "fixed":
            handshake["token_hashes"][str(source)] = "0" * 64
        if scenario == "wrong-version":
            handshake["version"] = "99.0.0"
        if scenario == "coverage-change" and label == "fixed":
            handshake["files"] = []
        probe = Path(os.environ["STATEMENT_SPACING_PROBE_DIR"]) / "test.json"
        probe.write_text(json.dumps(handshake))
    if scenario == "compiler-error":
        emit(
            {
                "reason": "compiler-message",
                "package_id": package_id,
                "message": {
                    "level": "error",
                    "message": "cannot resolve type",
                    "code": {"code": "E0412"},
                    "spans": [],
                    "children": [],
                },
            }
        )
        sys.exit(101)
    custom = os.environ.get("STATEMENT_SPACING_FAKE_DIAGNOSTICS")
    if custom is not None:
        if label == "initial":
            print(custom)
        sys.exit(0)
    data = source.read_bytes()
    has_gap = b";\n\n" in data
    needs_fix = not has_gap or scenario == "not-idempotent"
    if needs_fix:
        start = data.index(b";") + 1
        end = data.index(b"std::")
        replacement = "\n\n    "
        if scenario == "token-edit":
            replacement = "\n    unsafe {}\n    "
        primary = {
            "file_name": "src/lib.rs",
            "byte_start": start,
            "byte_end": end,
            "line_start": 2,
            "line_end": 3,
            "is_primary": True,
        }
        span = dict(
            primary,
            suggested_replacement=replacement,
            suggestion_applicability="MachineApplicable",
            is_primary=False,
        )
        diagnostic = {
            "level": "error" if scenario == "denied" else "warning",
            "message": "separate unrelated operations (test double)",
            "code": {"code": "statement_spacing_bindings"},
            "spans": [primary],
            "children": []
            if scenario == "no-fix"
            else [{"message": "insert gap", "spans": [span]}],
        }
        emit(
            {
                "reason": "compiler-message",
                "package_id": package_id,
                "message": diagnostic,
            }
        )
        if scenario in ("duplicate", "conflicting"):
            if scenario == "conflicting":
                diagnostic["children"][0]["spans"][0]["suggested_replacement"] = (
                    "\n\n\n    "
                )
            emit(
                {
                    "reason": "compiler-message",
                    "package_id": package_id,
                    "message": diagnostic,
                }
            )
        if scenario == "denied":
            sys.exit(101)
else:
    print("unexpected fake Cargo command: " + repr(args), file=sys.stderr)
    sys.exit(97)
