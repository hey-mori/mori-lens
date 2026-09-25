#!/usr/bin/env python3
"""
Public-readiness gate for Mori Lens repository.

Checks that the repository is ready for public release:
- Required public files exist
- AI-specific instructions removed
- No broken markdown links
- No obvious secrets or sensitive data
- Writing style compliant
- Version references coherent
"""

import os
import re
import sys
from pathlib import Path

# Force UTF-8 output for Windows
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

REPO_ROOT = Path(__file__).parent.parent
EXIT_CODE = 0


def fail(msg):
    global EXIT_CODE
    # Replace problematic Unicode characters for Windows console
    msg = msg.replace("⚠", "WARNING").replace("✓", "PASS").replace("✗", "FAIL")
    # Encode/decode to handle any remaining problematic characters
    try:
        msg = msg.encode("utf-8", errors="replace").decode("utf-8")
    except Exception:
        pass
    print(f"  FAIL: {msg}")
    EXIT_CODE = 1


def ok(msg):
    # Replace problematic Unicode characters for Windows console
    msg = msg.replace("⚠", "WARNING").replace("✓", "PASS").replace("✗", "FAIL")
    try:
        msg = msg.encode("utf-8", errors="replace").decode("utf-8")
    except Exception:
        pass
    print(f"  OK: {msg}")


def check_required_files():
    """Check that required public files exist."""
    print("\n· required public files")
    required = [
        "README.md",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "THIRD-PARTY-NOTICES.md",
        "docs/README.md",
    ]
    for f in required:
        path = REPO_ROOT / f
        if path.exists():
            ok(f)
        else:
            fail(f"{f} missing")


def check_forbidden_files():
    """Check that forbidden files are absent."""
    print("\n· forbidden files absent")
    forbidden = ["CLAUDE.md", "handoff.md"]
    for f in forbidden:
        path = REPO_ROOT / f
        if not path.exists():
            ok(f"{f} absent")
        else:
            fail(f"{f} should not exist")


def check_no_ai_attribution():
    """Check that AI attribution/promotional text is absent."""
    print("\n· no AI attribution in tracked files")
    patterns = [
        r"Generated with Claude",
        r"Co-Authored-By:.*Claude",
    ]
    found = []
    for path in REPO_ROOT.rglob("*"):
        if path.is_file() and not any(
            part in str(path)
            for part in [
                ".git",
                "node_modules",
                "target",
                "check_public_readiness.py",
                "sanitize_pr_metadata.py",
                "test-check-pr",
                "test_check_pr",
                "test_attribution",
            ]
        ):
            try:
                content = path.read_text(encoding="utf-8", errors="ignore")
                for pattern in patterns:
                    if re.search(pattern, content, re.IGNORECASE):
                        found.append(f"{path.relative_to(REPO_ROOT)}: {pattern}")
            except Exception:
                pass
    if not found:
        ok("no AI attribution found")
    else:
        for f in found[:5]:
            fail(f"AI attribution: {f}")
        if len(found) > 5:
            fail(f"... and {len(found) - 5} more")


def check_markdown_links():
    """Check that markdown links are not broken."""
    print("\n· markdown links")
    broken = []
    for md_file in REPO_ROOT.rglob("*.md"):
        # Skip build artifacts and dependencies
        if any(
            part in str(md_file)
            for part in [".git", "node_modules", "target", ".cargo"]
        ):
            continue
        try:
            content = md_file.read_text(encoding="utf-8", errors="ignore")
            # Find markdown links [text](path)
            for match in re.finditer(r"\[([^\]]+)\]\(([^)]+)\)", content):
                link = match.group(2)
                if link.startswith("http"):
                    continue
                if link.startswith("#"):
                    continue
                # Resolve relative to markdown file
                link_path = (md_file.parent / link).resolve()
                if not link_path.exists():
                    broken.append(f"{md_file.relative_to(REPO_ROOT)}: {link}")
        except Exception:
            pass
    if not broken:
        ok("all markdown links valid")
    else:
        for b in broken[:5]:
            fail(f"broken link: {b}")
        if len(broken) > 5:
            fail(f"... and {len(broken) - 5} more")


def check_em_dash():
    """Check that public docs don't contain em dashes."""
    print("\n· writing style (no em dashes in public docs)")
    public_docs = [
        "README.md",
        "CHANGELOG.md",
        "SECURITY.md",
        "THIRD-PARTY-NOTICES.md",
        "docs/README.md",
    ]
    found = []
    for doc in public_docs:
        path = REPO_ROOT / doc
        if path.exists():
            content = path.read_text(encoding="utf-8", errors="ignore")
            if "—" in content:
                found.append(doc)
    # CONTRIBUTING.md is allowed to have em dash in the writing style rule
    # (demonstrating what not to do)
    if not found:
        ok("no em dashes in public docs")
    else:
        for f in found:
            fail(f"em dash in {f}")


def check_version_consistency():
    """Check that version references are coherent."""
    print("\n· version consistency")
    # Check Cargo.toml
    cargo = REPO_ROOT / "Cargo.toml"
    if cargo.exists():
        content = cargo.read_text(encoding="utf-8", errors="ignore")
        if 'version = "0.1.0"' in content:
            ok("Cargo.toml version 0.1.0")
        else:
            fail("Cargo.toml version not 0.1.0")
    # Check CHANGELOG
    changelog = REPO_ROOT / "CHANGELOG.md"
    if changelog.exists():
        content = changelog.read_text(encoding="utf-8", errors="ignore")
        if "0.1.0" in content:
            ok("CHANGELOG.md references 0.1.0")
        else:
            fail("CHANGELOG.md does not reference 0.1.0")


def check_license_state():
    """Check license state consistency."""
    print("\n· license state")
    readme = REPO_ROOT / "README.md"
    if readme.exists():
        content = readme.read_text(encoding="utf-8", errors="ignore")
        if "Not licensed for use" in content or "all rights reserved" in content.lower():
            ok("README states all-rights-reserved")
        else:
            fail("README license state unclear")
    # Check that no LICENSE file exists (since we're all-rights-reserved)
    license_files = list(REPO_ROOT.glob("LICENSE*"))
    if not license_files:
        ok("no LICENSE file (consistent with all-rights-reserved)")
    else:
        fail(f"LICENSE file exists: {license_files[0].name}")


def check_no_secrets():
    """Check for obvious secrets or sensitive data."""
    print("\n· no obvious secrets")
    patterns = [
        (r"AKIA[0-9A-Z]{16}", "AWS access key"),
        (r"ghp_[0-9a-zA-Z]{36}", "GitHub personal access token"),
        (r"sk-[0-9a-zA-Z]{32,}", "API key"),
    ]
    found = []
    for path in REPO_ROOT.rglob("*"):
        if path.is_file() and not any(
            part in str(path) for part in [".git", "node_modules", "target"]
        ):
            try:
                content = path.read_text(encoding="utf-8", errors="ignore")
                for pattern, desc in patterns:
                    if re.search(pattern, content):
                        found.append(f"{path.relative_to(REPO_ROOT)}: {desc}")
            except Exception:
                pass
    if not found:
        ok("no obvious secrets found")
    else:
        for f in found[:5]:
            fail(f"possible secret: {f}")


def main():
    print("Public-readiness gate")
    print("=" * 60)

    check_required_files()
    check_forbidden_files()
    check_no_ai_attribution()
    check_markdown_links()
    check_em_dash()
    check_version_consistency()
    check_license_state()
    check_no_secrets()

    print("\n" + "=" * 60)
    if EXIT_CODE == 0:
        print("PUBLIC-READINESS GATE: PASS")
    else:
        print("PUBLIC-READINESS GATE: FAIL")

    sys.exit(EXIT_CODE)


if __name__ == "__main__":
    main()
