#!/usr/bin/env python3
"""Validate capability-evidence-matrix.md structure and counts.

This script mechanically parses the capability matrix and validates:
- Recognized evidence statuses
- Capability row counts
- Summary statistics match actual counts
- No duplicate capability names
- Well-formed table rows
"""

import re
import sys
from pathlib import Path
from collections import defaultdict

VALID_STATUSES = {
    "NOT IMPLEMENTED",
    "SOURCE VERIFIED",
    "AUTOMATED CONTRACT PASS",
    "PLATFORM BUILD VERIFIED",
    "RUNTIME VERIFIED",
    "HUMAN VERIFIED",
    "EXTERNAL BLOCKED",
    "NOT RUN ON CURRENT MAIN",
}

def parse_table_row(line):
    """Parse a markdown table row, returning cells or None if not a data row."""
    line = line.strip()
    if not line.startswith("|"):
        return None
    if re.match(r"^\|[-:\s|]+\|$", line):  # Header separator
        return None
    cells = [cell.strip() for cell in line.split("|")[1:-1]]
    return cells

def main():
    matrix_path = Path(__file__).parent.parent / "docs" / "capability-evidence-matrix.md"
    if not matrix_path.exists():
        print(f"ERROR: {matrix_path} not found", file=sys.stderr)
        return 1

    content = matrix_path.read_text(encoding="utf-8")
    lines = content.split("\n")

    capabilities = []
    status_counts = defaultdict(int)
    duplicates = []
    capability_names = set()
    errors = []

    in_capability_table = False
    current_section = None
    current_platform = None

    for i, line in enumerate(lines, 1):
        # Track section headers
        if line.startswith("## "):
            current_section = line[3:].strip()
            in_capability_table = False
            # Track platform for duplicate detection
            if "windows" in current_section.lower():
                current_platform = "windows"
            elif "macos" in current_section.lower():
                current_platform = "macos"
            continue

        # Parse table rows
        cells = parse_table_row(line)
        if cells is None:
            continue

        # Check if this is a capability table (4 columns: Capability | Status | Evidence | Notes)
        if len(cells) == 4 and current_section and "capabilities" in current_section.lower():
            capability = cells[0].strip()
            status = cells[1].strip()
            evidence = cells[2].strip()
            notes = cells[3].strip()

            # Skip header row
            if capability == "Capability" or status == "Status":
                in_capability_table = True
                continue

            # Validate status
            if status not in VALID_STATUSES:
                errors.append(f"Line {i}: Invalid status '{status}' for capability '{capability}'")
                continue

            # Check for duplicates (same name within same platform section)
            # Platform-specific duplicates are allowed (Windows vs macOS)
            platform_key = f"{current_platform}:{capability}" if current_platform else capability
            if platform_key in capability_names:
                duplicates.append(capability)
            capability_names.add(platform_key)

            capabilities.append({
                "name": capability,
                "status": status,
                "evidence": evidence,
                "notes": notes,
                "line": i,
            })
            status_counts[status] += 1

    # Parse summary section
    summary_match = re.search(
        r"## Summary statistics.*?- \*\*Total capabilities tracked:\*\* (\d+)(.*?)## ",
        content,
        re.DOTALL,
    )

    reported_total = None
    reported_counts = {}

    if summary_match:
        reported_total = int(summary_match.group(1))
        summary_text = summary_match.group(2)

        for status in VALID_STATUSES:
            match = re.search(rf"\*\*{re.escape(status)}:\*\* (\d+)", summary_text)
            if match:
                reported_counts[status] = int(match.group(1))

    # Validation
    actual_total = len(capabilities)
    validation_errors = []

    if reported_total is not None and reported_total != actual_total:
        validation_errors.append(
            f"Summary claims {reported_total} capabilities, but found {actual_total} actual capability rows"
        )

    for status in VALID_STATUSES:
        actual = status_counts[status]
        reported = reported_counts.get(status, 0)
        if actual != reported:
            validation_errors.append(
                f"Summary claims {reported} '{status}', but found {actual} actual rows"
            )

    if duplicates:
        validation_errors.append(f"Duplicate capability names: {', '.join(duplicates)}")

    # Report
    print("Capability Matrix Validation")
    print("=" * 60)
    print(f"Total capability rows: {actual_total}")
    print()
    print("Status counts:")
    for status in sorted(VALID_STATUSES):
        count = status_counts[status]
        print(f"  {status}: {count}")
    print()

    if errors:
        print("STRUCTURAL ERRORS:")
        for err in errors:
            print(f"  - {err}")
        print()

    if validation_errors:
        print("VALIDATION ERRORS (summary does not match actual):")
        for err in validation_errors:
            print(f"  - {err}")
        print()
        return 1

    if not errors and not validation_errors:
        print("[OK] All validations passed")
        print("[OK] Summary statistics match actual capability rows")
        return 0

    return 1 if (errors or validation_errors) else 0

if __name__ == "__main__":
    sys.exit(main())
