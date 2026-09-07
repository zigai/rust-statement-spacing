#!/usr/bin/env python3
"""Verify the actual Dylint registrations and group using the pinned compiler."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {
    "statement_spacing_" + name
    for name in (
        "bindings",
        "expressions",
        "control_flow",
        "after_block",
        "exit",
        "result_check",
        "item_spacing",
        "layout",
    )
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library-path", type=Path, default=ROOT / "lint")
    parser.add_argument(
        "--report", type=Path, default=ROOT / "validation-output/registration.json"
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=None,
        help="Target directory for Dylint library build artifacts",
    )
    options = parser.parse_args()
    report = {"kind": "compiler-lint-registration", "commands": []}

    def run(args, cwd, env=None):
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
        if process.returncode:
            raise RuntimeError(
                f"registration probe exited {process.returncode}: {process.stderr[-2000:]}"
            )
        return process.stdout

    try:
        library = options.library_path.resolve(strict=True)
        with tempfile.TemporaryDirectory(
            prefix="statement-spacing-registration-"
        ) as name:
            temporary = Path(name)
            fixture = temporary / "fixture"
            shutil.copytree(
                ROOT / "tests/workspaces/basic",
                fixture,
                ignore=shutil.ignore_patterns("target", ".statement-spacing"),
            )
            env = os.environ.copy()
            env.pop("DYLINT_RUSTFLAGS", None)
            env.pop("DYLINT_LIST", None)
            target = (options.target_dir or (temporary / "target")).resolve()
            env["CARGO_TARGET_DIR"] = str(target)
            listing = run(["cargo", "dylint", "list", "--path", library], fixture, env)
            registered = {
                parts[0]
                for line in listing.splitlines()
                if len(parts := line.split()) >= 2
                and parts[0].startswith("statement_spacing_")
            }
            if registered != EXPECTED:
                raise RuntimeError(f"registered lint IDs differ: {sorted(registered)}")
            # The list command builds the driver/library but omits groups.
            # Invoke that same driver directly: Cargo's metadata probes cannot
            # accept rustc's -W help output in place of --print=file-names.
            libraries = [
                path
                for path in target.glob(
                    "dylint/libraries/*/release/*statement_spacing@*"
                )
                if path.suffix in (".so", ".dylib", ".dll") and path.is_file()
            ]
            if not libraries:
                libraries = [
                    path
                    for path in target.glob(
                        "release/*statement_spacing@*"
                    )
                    if path.suffix in (".so", ".dylib", ".dll") and path.is_file()
                ]
            if len(libraries) != 1:
                raise RuntimeError(
                    "expected one freshly built statement_spacing library"
                )
            compiled = libraries[0]
            toolchain = compiled.stem.split("@", 1)[1]
            drivers = Path(
                env.get("DYLINT_DRIVER_PATH", Path.home() / ".dylint_drivers")
            )
            driver = drivers / toolchain / "dylint-driver"
            env["DYLINT_LIBS"] = json.dumps([str(compiled)])
            env["DYLINT_NO_DEPS"] = "0"
            help_output = run(
                ["rustup", "run", toolchain, driver, "rustc", "-W", "help"],
                fixture,
                env,
            )
            groups = [
                parts[1]
                for line in help_output.splitlines()
                if len(parts := line.split(maxsplit=1)) == 2
                and parts[0].replace("-", "_") == "statement_spacing"
            ]
            if len(groups) != 1:
                raise RuntimeError(
                    "compiler help did not report exactly one statement_spacing group"
                )
            members = {
                member.strip().replace("-", "_") for member in groups[0].split(",")
            }
            if members != EXPECTED:
                raise RuntimeError(f"registered lint group differs: {sorted(members)}")
            report.update(
                status="passed",
                registered_lint_ids=sorted(registered),
                group_members=sorted(members),
            )
            print(
                json.dumps(
                    {key: value for key, value in report.items() if key != "commands"},
                    indent=2,
                )
            )
            return 0
    except (OSError, RuntimeError, ValueError) as e:
        report.update(status="failed", error=str(e))
        print(f"registration validation failed: {e}")
        return 1
    finally:
        options.report.parent.mkdir(parents=True, exist_ok=True)
        options.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
