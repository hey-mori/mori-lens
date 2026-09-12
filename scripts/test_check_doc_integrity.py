#!/usr/bin/env python3
"""Regression tests for the documentation integrity checker."""

import os
import sys
import tempfile
import unittest
from pathlib import Path

# Import the checker functions
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from check_doc_integrity import (
    check_adr_index,
    check_markdown_links,
    extract_relative_links,
    resolve_link,
    adr_files,
    adr_index_entries,
)


class TestAdrIndex(unittest.TestCase):
    """Tests for ADR index completeness checking."""

    def setUp(self):
        """Create a temporary directory structure for testing."""
        self.tmpdir = tempfile.mkdtemp()
        self.root = Path(self.tmpdir)

        # Create docs/decisions directory
        self.adr_dir = self.root / "docs" / "decisions"
        self.adr_dir.mkdir(parents=True)

    def tearDown(self):
        """Clean up temporary directory."""
        import shutil
        shutil.rmtree(self.tmpdir)

    def _create_adr(self, name):
        """Create an ADR file."""
        (self.adr_dir / name).write_text(f"# ADR\n\nStatus: Accepted\n")

    def _create_readme(self, content):
        """Create the README.md with the given content."""
        (self.adr_dir / "README.md").write_text(content)

    def test_complete_index_passes(self):
        """A complete ADR index passes."""
        self._create_adr("ADR-0001-first.md")
        self._create_adr("ADR-0002-second.md")
        self._create_readme("""# ADRs

| # | Decision | Status |
|---|----------|--------|
| [ADR-0001](ADR-0001-first.md) | First | Accepted |
| [ADR-0002](ADR-0002-second.md) | Second | Accepted |
""")
        errors = check_adr_index(self.root)
        self.assertEqual(errors, [])

    def test_missing_index_entry_fails(self):
        """An ADR file missing from the index fails."""
        self._create_adr("ADR-0001-first.md")
        self._create_adr("ADR-0002-second.md")
        self._create_readme("""# ADRs

| # | Decision | Status |
|---|----------|--------|
| [ADR-0001](ADR-0001-first.md) | First | Accepted |
""")
        errors = check_adr_index(self.root)
        self.assertEqual(len(errors), 1)
        self.assertIn("ADR-0002-second.md", errors[0])
        self.assertIn("absent from", errors[0])

    def test_stale_index_entry_fails(self):
        """An index entry referencing a missing file fails."""
        self._create_adr("ADR-0001-first.md")
        self._create_readme("""# ADRs

| # | Decision | Status |
|---|----------|--------|
| [ADR-0001](ADR-0001-first.md) | First | Accepted |
| [ADR-0002](ADR-0002-nonexistent.md) | Second | Accepted |
""")
        errors = check_adr_index(self.root)
        self.assertEqual(len(errors), 1)
        self.assertIn("ADR-0002", errors[0])
        self.assertIn("does not exist", errors[0])

    def test_duplicate_entry_fails(self):
        """A duplicate ADR in the index fails."""
        self._create_adr("ADR-0001-first.md")
        self._create_readme("""# ADRs

| # | Decision | Status |
|---|----------|--------|
| [ADR-0001](ADR-0001-first.md) | First | Accepted |
| [ADR-0001](ADR-0001-first.md) | First again | Accepted |
""")
        errors = check_adr_index(self.root)
        self.assertEqual(len(errors), 1)
        self.assertIn("multiple times", errors[0])

    def test_empty_directory_passes(self):
        """No ADR files and empty index passes."""
        self._create_readme("""# ADRs

| # | Decision | Status |
|---|----------|--------|
""")
        errors = check_adr_index(self.root)
        self.assertEqual(errors, [])


class TestMarkdownLinks(unittest.TestCase):
    """Tests for relative markdown link checking."""

    def test_extract_relative_links(self):
        """Extract relative links from markdown text."""
        text = """
[relative](other.md)
[with anchor](other.md#section)
[external](https://example.com)
[mailto](mailto:test@example.com)
[pure anchor](#section)
[relative dir](subdir/)
"""
        source = Path("/tmp/test.md")
        links = extract_relative_links(text, source)

        targets = [t for t, _ in links]
        self.assertIn("other.md", targets)
        self.assertIn("other.md", targets)  # with anchor, file part extracted
        self.assertIn("subdir/", targets)
        self.assertNotIn("https://example.com", targets)
        self.assertNotIn("mailto:test@example.com", targets)
        self.assertNotIn("#section", targets)

    def test_extract_anchor_link(self):
        """Link with anchor extracts file component."""
        text = "[link](file.md#anchor)"
        source = Path("/tmp/test.md")
        links = extract_relative_links(text, source)
        self.assertEqual(len(links), 1)
        self.assertEqual(links[0][0], "file.md")

    def test_pure_anchor_skipped(self):
        """Pure anchor links are skipped."""
        text = "[section](#section)"
        source = Path("/tmp/test.md")
        links = extract_relative_links(text, source)
        self.assertEqual(len(links), 0)

    def test_resolve_parent_link(self):
        """Resolve a ../ link correctly."""
        source = Path("/repo/docs/file.md")
        root = Path("/repo")
        resolved = resolve_link(source, "../README.md", root)
        self.assertEqual(resolved, Path("/repo/README.md").resolve())

    def test_resolve_subdirectory_link(self):
        """Resolve a subdir/ link correctly."""
        source = Path("/repo/docs/file.md")
        root = Path("/repo")
        resolved = resolve_link(source, "decisions/ADR-0001.md", root)
        self.assertEqual(resolved, Path("/repo/docs/decisions/ADR-0001.md").resolve())


class TestCheckMarkdownLinksLogic(unittest.TestCase):
    """Tests for the link checking logic (no git dependency)."""

    def test_external_url_skipped(self):
        """External URLs are skipped."""
        text = "[external](https://example.com/page)"
        source = Path("/tmp/test.md")
        links = extract_relative_links(text, source)
        self.assertEqual(len(links), 0)

    def test_mailto_skipped(self):
        """Mailto links are skipped."""
        text = "[email](mailto:test@example.com)"
        source = Path("/tmp/test.md")
        links = extract_relative_links(text, source)
        self.assertEqual(len(links), 0)

    def test_directory_link_detected(self):
        """Directory links are detected."""
        text = "[dir](subdir/)"
        source = Path("/tmp/test.md")
        links = extract_relative_links(text, source)
        self.assertEqual(len(links), 1)
        self.assertEqual(links[0][0], "subdir/")

    def test_link_line_number_tracked(self):
        """Line numbers are tracked correctly."""
        text = "line 1\n[link](file.md)\nline 3"
        source = Path("/tmp/test.md")
        links = extract_relative_links(text, source)
        self.assertEqual(len(links), 1)
        self.assertEqual(links[0][1], 2)  # line 2


if __name__ == '__main__':
    unittest.main()
