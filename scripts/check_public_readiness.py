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


def check_node_project_paths():
    """Check that Node project paths are correct."""
    print("\n· Node project paths")
    # Check that CONTRIBUTING.md uses correct npm command paths
    contributing = REPO_ROOT / "CONTRIBUTING.md"
    if contributing.exists():
        content = contributing.read_text(encoding="utf-8", errors="ignore")
        # Check for stale npm command paths (npm ci/test with --prefix apps/lens/web)
        if re.search(r"npm\s+(ci|test|run).*--prefix\s+apps/lens/web", content):
            fail("CONTRIBUTING.md contains stale npm command path 'apps/lens/web'")
        elif re.search(r"npm\s+(ci|test|run).*--prefix\s+apps/lens", content):
            ok("CONTRIBUTING.md uses correct npm command path 'apps/lens'")
        else:
            ok("CONTRIBUTING.md npm paths appear correct")
    else:
        fail("CONTRIBUTING.md missing")

    # Check workflows for stale npm paths
    workflows_dir = REPO_ROOT / ".github" / "workflows"
    if workflows_dir.exists():
        for workflow in workflows_dir.glob("*.yml"):
            content = workflow.read_text(encoding="utf-8", errors="ignore")
            if re.search(r"npm\s+(ci|test|run).*--prefix\s+apps/lens/web", content):
                fail(f"{workflow.name} contains stale npm command path 'apps/lens/web'")

    # Check scripts for stale npm paths
    scripts_dir = REPO_ROOT / "scripts"
    if scripts_dir.exists():
        for script in scripts_dir.glob("*.sh"):
            content = script.read_text(encoding="utf-8", errors="ignore")
            if re.search(r"npm\s+(ci|test|run).*--prefix\s+apps/lens/web", content):
                fail(f"{script.name} contains stale npm command path 'apps/lens/web'")

    # Verify actual package.json location
    package_json = REPO_ROOT / "apps/lens/package.json"
    if package_json.exists():
        ok("apps/lens/package.json exists")
    else:
        fail("apps/lens/package.json missing")


def check_release_workflow_no_smoke_assets():
    """Check that release workflow does not use smoke assets."""
    print("\n· Release workflow asset policy")
    release_yml = REPO_ROOT / ".github" / "workflows" / "release.yml"
    if release_yml.exists():
        content = release_yml.read_text(encoding="utf-8", errors="ignore")
        # Check for smoke asset generation in release workflow
        if "generate-smoke-assets.py" in content:
            fail("release.yml invokes generate-smoke-assets.py (must use production assets only)")
        else:
            ok("release.yml does not invoke smoke asset generator")

        # Check for production asset verification
        if "Verify production assets" in content or "production asset" in content.lower():
            ok("release.yml verifies production assets")
        else:
            fail("release.yml should verify production assets are present")
    else:
        ok("release.yml not present (OK for now)")


def check_changelog_unreleased():
    """Check that CHANGELOG uses Unreleased until actual release."""
    print("\n· CHANGELOG unreleased status")
    changelog = REPO_ROOT / "CHANGELOG.md"
    if not changelog.exists():
        fail("CHANGELOG.md missing")
        return

    content = changelog.read_text(encoding="utf-8", errors="ignore")
    # Check for fake release dates
    if re.search(r"## \[0\.1\.0\] - \d{4}-\d{2}-\d{2}", content):
        fail("CHANGELOG.md has fake release date")
    elif "## [Unreleased]" in content:
        ok("CHANGELOG.md uses [Unreleased]")
    else:
        fail("CHANGELOG.md missing [Unreleased] section")

    # Check for release links that don't exist yet
    if "releases/tag/v0.1.0" in content:
        fail("CHANGELOG.md links to non-existent v0.1.0 release")
    else:
        ok("CHANGELOG.md does not link to non-existent releases")


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
    check_node_project_paths()
    check_changelog_unreleased()
    check_release_workflow_no_smoke_assets()

    print("\n" + "=" * 60)
    if EXIT_CODE == 0:
        print("PUBLIC-READINESS GATE: PASS")
    else:
        print("PUBLIC-READINESS GATE: FAIL")

    sys.exit(EXIT_CODE)


if __name__ == "__main__":
    main()
