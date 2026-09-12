#!/usr/bin/env python3
"""Documentation integrity checker.

Detects three classes of documentation drift:

1. ADR index completeness: every docs/decisions/ADR-*.md file appears
   exactly once in docs/decisions/README.md; no stale entries reference
   missing files.

2. Relative markdown links: every relative link in a tracked .md file
   resolves to an existing path. External URLs, pure anchors, and
   GitHub shorthand are ignored.

3. Duplicate index entries: no ADR number appears twice in the index.

Uses only the Python standard library. Designed for the fast gate.
"""

import os
import re
import sys
from pathlib import Path, PurePosixPath


def repo_root():
    """Return the repository root (parent of scripts/)."""
    return Path(__file__).resolve().parent.parent


def adr_files(root):
    """Return sorted set of ADR filenames in docs/decisions/."""
    adr_dir = root / "docs" / "decisions"
    if not adr_dir.is_dir():
        return set()
    return {f.name for f in adr_dir.iterdir()
            if f.name.startswith("ADR-") and f.name.endswith(".md")}


def adr_index_entries(root):
    """Parse the ADR index table in docs/decisions/README.md.

    Returns a list of (adr_number, filename) tuples from table rows
    matching: | [ADR-NNNN](ADR-NNNN-....md) | ... |
    """
    readme = root / "docs" / "decisions" / "README.md"
    if not readme.is_file():
        return []
    text = readme.read_text(encoding="utf-8")
    # Match rows like: | [ADR-0001](ADR-0001-independent-repository.md) | ... |
    pattern = re.compile(
        r'\|\s*\[ADR-(\d+)\]\(([^)]+)\)\s*\|'
    )
    return [(m.group(1), m.group(2)) for m in pattern.finditer(text)]


def check_adr_index(root):
    """Check ADR index completeness and duplicates.

    Returns a list of error strings. Empty list means pass.
    """
    errors = []
    actual = adr_files(root)
    entries = adr_index_entries(root)

    # Check for duplicates
    seen = {}
    for num, filename in entries:
        if num in seen:
            errors.append(
                f"ADR-{num} appears multiple times in docs/decisions/README.md"
            )
        seen[num] = filename

    # Check that every actual ADR file is in the index
    indexed_files = {filename for _, filename in entries}
    for f in sorted(actual):
        if f not in indexed_files:
            errors.append(
                f"{f} exists but is absent from docs/decisions/README.md"
            )

    # Check that every index entry refers to an actual file
    for num, filename in entries:
        if filename not in actual:
            errors.append(
                f"ADR-{num} in index references {filename} which does not exist"
            )

    return errors


def extract_relative_links(text, source_path):
    """Extract relative links from markdown text.

    Returns a list of (link_target, line_number) tuples.
    Ignores http://, https://, mailto:, and pure #anchor links.
    """
    links = []
    # Match markdown links: [text](target)
    # Also match HTML img src and other common patterns
    link_pattern = re.compile(r'\]\(([^)]+)\)')

    for line_num, line in enumerate(text.split('\n'), start=1):
        for match in link_pattern.finditer(line):
            target = match.group(1).strip()

            # Skip external URLs
            if target.startswith(('http://', 'https://', 'mailto:')):
                continue

            # Skip pure anchor links
            if target.startswith('#'):
                continue

            # Strip anchor fragment for file validation
            file_part = target.split('#')[0]

            # Skip if no file part (was a pure anchor)
            if not file_part:
                continue

            links.append((file_part, line_num))

    return links


def resolve_link(source_path, link_target, root):
    """Resolve a relative link to an absolute path.

    Returns the resolved Path, or None if the link cannot be resolved.
    """
    # Normalize the link target to use forward slashes
    normalized = link_target.replace('\\', '/')

    # Resolve relative to the source file's directory
    source_dir = source_path.parent
    resolved = (source_dir / normalized).resolve()

    return resolved


def check_markdown_links(root):
    """Check relative markdown links in all tracked .md files.

    Returns a list of error strings. Empty list means pass.
    """
    errors = []

    # Get all tracked .md files using git ls-files
    import subprocess
    try:
        result = subprocess.run(
            ['git', 'ls-files', '*.md'],
            capture_output=True, text=True, cwd=root
        )
        if result.returncode != 0:
            return [f"git ls-files failed: {result.stderr}"]
        md_files = [root / f for f in result.stdout.strip().split('\n') if f]
    except FileNotFoundError:
        return ["git not found; cannot check markdown links"]

    for md_file in md_files:
        if not md_file.is_file():
            continue

        text = md_file.read_text(encoding='utf-8')
        links = extract_relative_links(text, md_file)

        for link_target, line_num in links:
            resolved = resolve_link(md_file, link_target, root)

            # Check if the target exists
            # For directory links (ending with /), check if it's a directory
            if link_target.endswith('/'):
                if not resolved.is_dir():
                    rel_path = md_file.relative_to(root)
                    errors.append(
                        f"{rel_path}:{line_num}: broken directory link -> {link_target}"
                    )
            else:
                if not resolved.is_file():
                    rel_path = md_file.relative_to(root)
                    errors.append(
                        f"{rel_path}:{line_num}: broken relative link -> {link_target}"
                    )

    return errors


def main():
    """Run all documentation integrity checks."""
    root = repo_root()
    all_errors = []

    # ADR index check
    adr_errors = check_adr_index(root)
    all_errors.extend(adr_errors)

    # Markdown link check
    link_errors = check_markdown_links(root)
    all_errors.extend(link_errors)

    if all_errors:
        print("Documentation integrity check FAILED:", file=sys.stderr)
        for err in all_errors:
            print(f"  {err}", file=sys.stderr)
        sys.exit(1)
    else:
        print("Documentation integrity check passed.")
        sys.exit(0)


if __name__ == '__main__':
    main()
