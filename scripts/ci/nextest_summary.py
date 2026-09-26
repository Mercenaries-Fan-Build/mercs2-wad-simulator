#!/usr/bin/env python3
"""Roll a cargo-nextest JUnit report up into a per-crate Markdown table.

CI runs `cargo nextest run --profile ci`, which writes target/nextest/ci/junit.xml.
nextest names each <testsuite> `crate-name::binary-name`, so the segment before the
first `::` is the crate. This script groups every <testcase> by that crate and prints
a Markdown table (Passed / Failed / Skipped / Total per crate) to stdout, which the
workflow appends to $GITHUB_STEP_SUMMARY.

It is a reporter, not a gate: the nextest step already fails the job on any test
failure. The only thing this script hard-fails on is a missing or unparseable report,
because that means the run produced no results to attribute — a real problem, not
something to paper over.

Usage: nextest_summary.py <path-to-junit.xml>
"""
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <path-to-junit.xml>", file=sys.stderr)
        return 2

    junit_path = sys.argv[1]

    try:
        tree = ET.parse(junit_path)
    except FileNotFoundError:
        print(
            f"ERROR: {junit_path} not found — nextest produced no JUnit report. "
            "This usually means the test build failed to compile before any test ran.",
            file=sys.stderr,
        )
        return 1
    except ET.ParseError as exc:
        print(f"ERROR: {junit_path} is not valid XML: {exc}", file=sys.stderr)
        return 1

    root = tree.getroot()

    # crate -> {"passed", "failed", "skipped"}
    crates: dict[str, dict[str, int]] = defaultdict(
        lambda: {"passed": 0, "failed": 0, "skipped": 0}
    )

    # nextest nests <testsuite> under the <testsuites> root. Each testsuite's name is
    # "crate-name::binary-name"; the crate is the part before the first "::".
    for suite in root.iter("testsuite"):
        suite_name = suite.get("name", "")
        crate = suite_name.split("::", 1)[0] or "(unnamed)"
        for case in suite.findall("testcase"):
            if case.find("failure") is not None or case.find("error") is not None:
                crates[crate]["failed"] += 1
            elif case.find("skipped") is not None:
                crates[crate]["skipped"] += 1
            else:
                crates[crate]["passed"] += 1

    if not crates:
        print(
            f"ERROR: {junit_path} contained no test cases — nothing was exercised.",
            file=sys.stderr,
        )
        return 1

    total_pass = sum(c["passed"] for c in crates.values())
    total_fail = sum(c["failed"] for c in crates.values())
    total_skip = sum(c["skipped"] for c in crates.values())

    overall = "❌ failed" if total_fail else "✅ passed"
    lines = [
        "## Workspace tests — per crate",
        "",
        f"**{overall}** · {len(crates)} crates · "
        f"{total_pass} passed, {total_fail} failed, {total_skip} skipped",
        "",
        "| Crate | Passed | Failed | Skipped | Total |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]

    # Failing crates first (so the eye lands on them), then alphabetical.
    for crate in sorted(crates, key=lambda c: (crates[c]["failed"] == 0, c)):
        counts = crates[crate]
        total = counts["passed"] + counts["failed"] + counts["skipped"]
        mark = " ❌" if counts["failed"] else ""
        lines.append(
            f"| `{crate}`{mark} | {counts['passed']} | {counts['failed']} "
            f"| {counts['skipped']} | {total} |"
        )

    lines.append(
        f"| **total** | **{total_pass}** | **{total_fail}** "
        f"| **{total_skip}** | **{total_pass + total_fail + total_skip}** |"
    )

    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
