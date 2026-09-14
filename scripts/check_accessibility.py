#!/usr/bin/env python3
"""
Validate accessibility attributes in HTML files.

Checks:
- All label[for] targets exist
- No duplicate IDs
- aria-controls points to existing elements
- aria-labelledby points to existing elements
- No positive tabindex (only 0 or -1)
- All role="tab" have aria-selected
- All role="tabpanel" have aria-labelledby
- Skip link target exists
- All IDs are unique

Exit 0 if all checks pass, exit 1 with diagnostics otherwise.
"""

import re
import sys
from pathlib import Path
from html.parser import HTMLParser


class AccessibilityValidator(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = {}  # id -> line number
        self.label_fors = []  # (for_value, line_number)
        self.aria_controls = []  # (value, line_number)
        self.aria_labelledby = []  # (value, line_number)
        self.tabindexes = []  # (value, line_number)
        self.tabs = []  # line numbers of role="tab"
        self.tabpanels = []  # line numbers of role="tabpanel"
        self.skip_link_target = None
        self.errors = []

    def handle_starttag(self, tag, attrs):
        line = self.getpos()[0]
        attrs_dict = dict(attrs)

        # Check for ID
        if 'id' in attrs_dict:
            id_val = attrs_dict['id']
            if id_val in self.ids:
                self.errors.append(f"Line {line}: Duplicate ID '{id_val}' (first seen at line {self.ids[id_val]})")
            else:
                self.ids[id_val] = line

        # Check for skip link
        if tag == 'a' and 'href' in attrs_dict:
            href = attrs_dict['href']
            if href.startswith('#'):
                target_id = href[1:]
                if target_id == 'main-content':
                    self.skip_link_target = target_id

        # Check label[for]
        if tag == 'label' and 'for' in attrs_dict:
            self.label_fors.append((attrs_dict['for'], line))

        # Check aria-controls
        if 'aria-controls' in attrs_dict:
            self.aria_controls.append((attrs_dict['aria-controls'], line))

        # Check aria-labelledby
        if 'aria-labelledby' in attrs_dict:
            self.aria_labelledby.append((attrs_dict['aria-labelledby'], line))

        # Check tabindex
        if 'tabindex' in attrs_dict:
            tabindex_val = attrs_dict['tabindex']
            self.tabindexes.append((tabindex_val, line))

        # Check role="tab" has aria-selected
        if attrs_dict.get('role') == 'tab':
            if 'aria-selected' not in attrs_dict:
                self.errors.append(f"Line {line}: role='tab' missing aria-selected")
            self.tabs.append(line)

        # Check role="tabpanel" has aria-labelledby
        if attrs_dict.get('role') == 'tabpanel':
            if 'aria-labelledby' not in attrs_dict:
                self.errors.append(f"Line {line}: role='tabpanel' missing aria-labelledby")
            self.tabpanels.append(line)

    def validate_references(self):
        """Validate that all references point to existing IDs."""
        # Check label[for] targets
        for for_val, line in self.label_fors:
            if for_val not in self.ids:
                self.errors.append(f"Line {line}: label[for='{for_val}'] targets non-existent ID")

        # Check aria-controls targets
        for value, line in self.aria_controls:
            # aria-controls can have multiple IDs separated by space
            for id_ref in value.split():
                if id_ref not in self.ids:
                    self.errors.append(f"Line {line}: aria-controls='{value}' references non-existent ID '{id_ref}'")

        # Check aria-labelledby targets
        for value, line in self.aria_labelledby:
            # aria-labelledby can have multiple IDs separated by space
            for id_ref in value.split():
                if id_ref not in self.ids:
                    self.errors.append(f"Line {line}: aria-labelledby='{value}' references non-existent ID '{id_ref}'")

        # Check skip link target
        if self.skip_link_target and self.skip_link_target not in self.ids:
            self.errors.append(f"Skip link target '#{self.skip_link_target}' does not exist")

        # Check tabindex values
        for value, line in self.tabindexes:
            try:
                tabindex_int = int(value)
                if tabindex_int > 0:
                    self.errors.append(f"Line {line}: Positive tabindex '{value}' (only 0 or -1 allowed)")
            except ValueError:
                self.errors.append(f"Line {line}: Invalid tabindex '{value}'")


def validate_file(filepath):
    """Validate a single HTML file."""
    print(f"Validating {filepath}...")

    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    validator = AccessibilityValidator()
    try:
        validator.feed(content)
        validator.validate_references()
    except Exception as e:
        print(f"ERROR: Failed to parse {filepath}: {e}")
        return False

    if validator.errors:
        print(f"FAIL: {filepath}")
        for error in validator.errors:
            print(f"  - {error}")
        return False

    print(f"OK: {filepath}")
    print(f"  - {len(validator.ids)} unique IDs")
    print(f"  - {len(validator.label_fors)} label[for] references")
    print(f"  - {len(validator.aria_controls)} aria-controls references")
    print(f"  - {len(validator.aria_labelledby)} aria-labelledby references")
    print(f"  - {len(validator.tabs)} tabs with aria-selected")
    print(f"  - {len(validator.tabpanels)} tabpanels with aria-labelledby")
    return True


def main():
    repo_root = Path(__file__).parent.parent
    index_html = repo_root / 'apps' / 'lens' / 'web' / 'index.html'

    if not index_html.exists():
        print(f"ERROR: {index_html} not found")
        sys.exit(1)

    success = validate_file(index_html)

    if success:
        print("\na11y-check: all accessibility checks passed")
        sys.exit(0)
    else:
        print("\na11y-check: FAILED")
        sys.exit(1)


if __name__ == '__main__':
    main()
