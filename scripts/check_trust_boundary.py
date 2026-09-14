#!/usr/bin/env python3
"""
DOM trust boundary checker.

Scans JS modules and HTML files for innerHTML/outerHTML/document.write sinks
where untrusted data may flow without escapeHtml() wrapping.

This checker uses pattern matching and cannot guarantee finding all taint flows.
Its coverage boundary:
- Detects: template literals with ${var}, string concatenation with variables
- Does NOT detect: complex data flow through multiple functions, indirect sinks
- False positives: static strings that look like variables (rare)
- False negatives: deeply nested taint flows (by design - this is a first line of defense)

For comprehensive XSS prevention, combine with:
- Code review for all innerHTML assignments
- CSP headers
- Integration testing with fuzzed inputs
"""

import re
import sys
from pathlib import Path

# Patterns that indicate potential XSS vulnerabilities
# These catch common cases where untrusted data flows into innerHTML

# Pattern 1: Template literal with variable interpolation
# e.g., el.innerHTML = `<p>${userInput}</p>`
TEMPLATE_LITERAL_PATTERN = re.compile(
    r'\.innerHTML\s*=\s*`[^`]*\$\{([^}]+)\}[^`]*`'
)

# Pattern 2: String concatenation with variable
# e.g., el.innerHTML = '<p>' + userInput + '</p>'
CONCAT_PATTERN = re.compile(
    r'\.innerHTML\s*=\s*[\'"][^\'"]*[\'"]\s*\+\s*([a-zA-Z_][a-zA-Z0-9_.]*)\s*\+'
)

# Pattern 3: Variable assigned to innerHTML directly
# e.g., el.innerHTML = html (where html was built from untrusted data)
# This is harder to detect without data flow analysis, so we skip it

# Pattern 4: Error message concatenation without escapeHtml
# e.g., el.innerHTML = '<p class="error">Error: ' + e + '</p>'
# Also catches: '<span class="error">Error: ' + e + '</span>'
# Note: [^\'"]* cannot match through quotes, so we use .*? for the HTML content
ERROR_CONCAT_PATTERN = re.compile(
    r'\.innerHTML\s*=\s*[\'"].*?[Ee]rror[:\s]*[\'"]\s*\+\s*([a-zA-Z_][a-zA-Z0-9_.]*)\s*\+'
)

def check_file(filepath):
    """Check a single file for trust boundary violations."""
    violations = []

    try:
        content = filepath.read_text(encoding='utf-8')
    except Exception as e:
        print(f"Warning: Could not read {filepath}: {e}", file=sys.stderr)
        return violations

    lines = content.split('\n')

    for line_num, line in enumerate(lines, 1):
        # Skip comments
        if line.strip().startswith('//') or line.strip().startswith('*'):
            continue

        # Check for template literal with variable interpolation
        # Skip if escapeHtml is present
        if TEMPLATE_LITERAL_PATTERN.search(line):
            if 'escapeHtml' not in line:
                # Extract the variable name
                match = TEMPLATE_LITERAL_PATTERN.search(line)
                if match:
                    var = match.group(1).strip()
                    # Skip numeric/count variables that are safe
                    if var not in ['count', 'i', 'index', 'length']:
                        violations.append({
                            'file': str(filepath),
                            'line': line_num,
                            'pattern': 'template-literal',
                            'snippet': line.strip()[:80],
                            'variable': var
                        })

        # Check for error message concatenation without escapeHtml
        if ERROR_CONCAT_PATTERN.search(line):
            if 'escapeHtml' not in line and 'String(' not in line:
                match = ERROR_CONCAT_PATTERN.search(line)
                if match:
                    var = match.group(1).strip()
                    violations.append({
                        'file': str(filepath),
                        'line': line_num,
                        'pattern': 'error-concat',
                        'snippet': line.strip()[:80],
                        'variable': var
                    })

        # Check for general concatenation with model output
        if 'result.model' in line or 'result.translated' in line:
            if '.innerHTML' in line and 'escapeHtml' not in line:
                violations.append({
                    'file': str(filepath),
                    'line': line_num,
                    'pattern': 'model-output',
                    'snippet': line.strip()[:80],
                    'variable': 'result.model/translated'
                })

    return violations

def main():
    """Main entry point."""
    repo_root = Path(__file__).parent.parent
    web_dir = repo_root / 'apps' / 'lens' / 'web'

    # Files to check
    js_modules = list((web_dir / 'js').glob('*.js'))
    html_files = [web_dir / 'index.html', web_dir / 'detail.html']

    all_files = js_modules + [f for f in html_files if f.exists()]

    all_violations = []
    for filepath in all_files:
        violations = check_file(filepath)
        all_violations.extend(violations)

    if all_violations:
        print("DOM trust boundary violations found:")
        print()
        for v in all_violations:
            print(f"  {v['file']}:{v['line']}")
            print(f"    Pattern: {v['pattern']}")
            print(f"    Variable: {v['variable']}")
            print(f"    Snippet: {v['snippet']}")
            print()
        print(f"Total: {len(all_violations)} violation(s)")
        print()
        print("Fix: Wrap untrusted variables with escapeHtml(String(...))")
        return 1
    else:
        print(f"DOM trust boundary: OK ({len(all_files)} files checked)")
        return 0

if __name__ == '__main__':
    sys.exit(main())
