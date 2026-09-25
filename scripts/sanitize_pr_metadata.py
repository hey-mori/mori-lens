#!/usr/bin/env python3
"""
Sanitize GitHub PR bodies by removing AI attribution footers.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

def run_gh(args):
    """Run gh CLI command and return output."""
    result = subprocess.run(
        ["gh"] + args,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        print(f"ERROR: gh {' '.join(args)} failed: {result.stderr}", file=sys.stderr)
        return None
    return result.stdout

def sanitize_body(body):
    """Remove AI attribution from PR body."""
    if not body:
        return body

    original = body

    # Remove "🤖 Generated with [Claude Code](...)" lines (with optional preceding ---)
    # Pattern 1: ---\n\n🤖 Generated with...
    body = re.sub(
        r'\n*---\s*\n*🤖\s*Generated with \[Claude Code\]\([^\)]+\)\s*$',
        '',
        body,
        flags=re.MULTILINE | re.IGNORECASE
    )

    # Pattern 2: Just the footer without ---
    body = re.sub(
        r'\n*🤖\s*Generated with \[Claude Code\]\([^\)]+\)\s*$',
        '',
        body,
        flags=re.MULTILINE | re.IGNORECASE
    )

    # Remove "Co-authored-by:" lines (any AI tool)
    body = re.sub(
        r'\n*Co-authored-by:\s*[^\n]+\n*',
        '\n',
        body,
        flags=re.IGNORECASE
    )

    # Remove "Generated with Claude" (without the link)
    body = re.sub(
        r'\n*Generated with Claude[^\n]*\n*',
        '\n',
        body,
        flags=re.IGNORECASE
    )

    # Clean up excessive blank lines (more than 2 in a row)
    body = re.sub(r'\n{3,}', '\n\n', body)

    # Strip trailing whitespace/newlines
    body = body.rstrip() + '\n' if body.strip() else ''

    return body

def main():
    print("Fetching all PRs with AI attribution...")

    # Get all PRs
    output = run_gh([
        "pr", "list", "--state", "all", "--limit", "1000",
        "--json", "number,title,body"
    ])

    if not output:
        print("Failed to fetch PRs")
        sys.exit(1)

    prs = json.loads(output)

    # Filter PRs that need sanitization
    prs_to_fix = []
    for pr in prs:
        body = pr.get("body", "")
        if not body:
            continue

        # Check if body has AI attribution
        has_generated = bool(re.search(r"Generated with.*Claude", body, re.IGNORECASE))
        has_coauthored = bool(re.search(r"^Co-authored-by:", body, re.MULTILINE | re.IGNORECASE))

        if has_generated or has_coauthored:
            prs_to_fix.append(pr)

    print(f"Found {len(prs_to_fix)} PRs with AI attribution")
    print()

    # Sanitize each PR
    sanitized = 0
    for pr in prs_to_fix:
        number = pr["number"]
        title = pr["title"]
        body = pr.get("body", "")

        new_body = sanitize_body(body)

        if new_body != body:
            # Update the PR
            print(f"Sanitizing PR #{number}: {title}")

            # Write new body to temp file to avoid shell escaping issues
            temp_file = Path(f"/tmp/pr_{number}_body.md")
            temp_file.write_text(new_body, encoding="utf-8")

            # Use gh to update the PR body from file
            result = subprocess.run(
                ["gh", "pr", "edit", str(number), "--body-file", str(temp_file)],
                capture_output=True,
                text=True,
            )

            if result.returncode == 0:
                sanitized += 1
                temp_file.unlink(missing_ok=True)
            else:
                print(f"  ERROR: Failed to update PR #{number}: {result.stderr}")

    print()
    print(f"Sanitized {sanitized} PRs")

if __name__ == "__main__":
    main()
