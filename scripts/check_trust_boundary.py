#!/usr/bin/env python3
"""
Check for DOM trust boundary violations.

Scans JavaScript files for innerHTML assignments that use unescaped data.
Reports potential XSS vulnerabilities where untrusted data flows into DOM sinks.

Exit 0 if no violations found, exit 1 with diagnostics otherwise.
"""

import re
import sys
from pathlib import Path


def check_file(filepath):
    """Check a single file for trust boundary violations."""
    print(f"Checking {filepath}...")

    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    violations = []

    # Pattern 1: innerHTML with template literals containing variables
    # e.g., innerHTML = `<p>${variable}</p>`
    template_pattern = re.compile(r'\.innerHTML\s*=\s*`[^`]*\$\{([^}]+)\}[^`]*`')
    for match in template_pattern.finditer(content):
        var_expr = match.group(1)
        # Check if escapeHtml is applied
        if 'escapeHtml' not in var_expr:
            line_num = content[:match.start()].count('\n') + 1
            violations.append(f"Line {line_num}: Unescaped template variable in innerHTML: {var_expr}")

    # Pattern 2: innerHTML with string concatenation
    # e.g., innerHTML = '<p>' + variable + '</p>'
    # This is harder to detect reliably, so we look for common patterns
    concat_pattern = re.compile(r'\.innerHTML\s*=\s*["\'][^"\']*["\']\s*\+\s*([^+]+)\s*\+\s*["\']')
    for match in concat_pattern.finditer(content):
        var_expr = match.group(1).strip()
        # Skip if it's a function call or already escaped
        if '(' in var_expr or 'escapeHtml' in var_expr:
            continue
        # Skip numeric variables (counts, IDs, etc.)
        if re.match(r'^\w+(Count|Id|Index|Offset|Length|Total)$', var_expr):
            continue
        line_num = content[:match.start()].count('\n') + 1
        violations.append(f"Line {line_num}: Potentially unescaped concatenation in innerHTML: {var_expr}")

    if violations:
        print(f"FAIL: {filepath}")
        for v in violations:
            print(f"  - {v}")
        return False

    print(f"OK: {filepath}")
    return True


def main():
    repo_root = Path(__file__).parent.parent
    web_dir = repo_root / 'apps' / 'lens' / 'web'

    # Check inline JavaScript in HTML files
    html_files = [
        web_dir / 'index.html',
        web_dir / 'detail.html',
    ]

    all_success = True
    for html_file in html_files:
        if not html_file.exists():
            print(f"WARNING: {html_file} not found, skipping")
            continue
        success = check_file(html_file)
        if not success:
            all_success = False

    # Check separate JS modules if they exist
    js_dir = web_dir / 'js'
    if js_dir.exists():
        for js_file in js_dir.glob('*.js'):
            success = check_file(js_file)
            if not success:
                all_success = False

    if all_success:
        print("\ntrust-boundary-check: all checks passed")
        sys.exit(0)
    else:
        print("\ntrust-boundary-check: FAILED")
        print("\nREMEDIATION: Use escapeHtml() for all user-controlled data before inserting into innerHTML.")
        print("Example: innerHTML = '<p>' + escapeHtml(userInput) + '</p>'")
        sys.exit(1)


if __name__ == '__main__':
    main()
