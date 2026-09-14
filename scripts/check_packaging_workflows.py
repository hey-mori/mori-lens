#!/usr/bin/env python3
"""
Mori Lens -- Workflow contract regression checker.

Verifies that release.yml and package-smoke.yml follow the proven
package contract:
- Tauri builds scoped to apps/lens working directory
- NSIS verifier receives -PayloadExePath and -ExpectedVersion
- Exact versioned NSIS artifact selected (not arbitrary first .exe)

This is a static text check, not a full YAML parser. It catches the
most likely regressions without requiring PyYAML or similar dependencies.

Exit 0 if all contracts satisfied, exit 1 if any contract violated.
"""

import re
import sys
from pathlib import Path


def check_workflow_contract(workflow_path: Path) -> list[str]:
    """Check a single workflow file for contract violations."""
    errors = []

    if not workflow_path.exists():
        return [f"{workflow_path}: file not found"]

    content = workflow_path.read_text(encoding="utf-8")

    # Check 1: NSIS Tauri build must have working-directory: apps/lens
    # Look for "cargo tauri build --bundles nsis" and verify it's scoped
    nsis_build_pattern = r"name:\s*Build NSIS installer.*?run:\s*cargo tauri build --bundles nsis"
    nsis_builds = re.findall(nsis_build_pattern, content, re.DOTALL | re.IGNORECASE)

    for build_block in nsis_builds:
        if "working-directory: apps/lens" not in build_block:
            errors.append(
                f"{workflow_path.name}: NSIS Tauri build must use "
                f"working-directory: apps/lens"
            )

    # Check 2: macOS Tauri build must have working-directory: apps/lens
    macos_build_pattern = r"name:\s*Build \.app bundle.*?run:\s*cargo tauri build --bundles app"
    macos_builds = re.findall(macos_build_pattern, content, re.DOTALL | re.IGNORECASE)

    for build_block in macos_builds:
        if "working-directory: apps/lens" not in build_block:
            errors.append(
                f"{workflow_path.name}: macOS Tauri build must use "
                f"working-directory: apps/lens"
            )

    # Check 3: NSIS verifier must receive -PayloadExePath
    # Look for verify-nsis.ps1 invocations
    nsis_verify_pattern = r"verify-nsis\.ps1.*?(?=\n\s*-\s*name:|\Z)"
    nsis_verifications = re.findall(nsis_verify_pattern, content, re.DOTALL)

    for verify_block in nsis_verifications:
        if "-PayloadExePath" not in verify_block and "PayloadExePath" not in verify_block:
            errors.append(
                f"{workflow_path.name}: NSIS verifier must receive -PayloadExePath"
            )

    # Check 4: NSIS verifier must receive -ExpectedVersion
    for verify_block in nsis_verifications:
        if "-ExpectedVersion" not in verify_block and "ExpectedVersion" not in verify_block:
            errors.append(
                f"{workflow_path.name}: NSIS verifier must receive -ExpectedVersion"
            )

    # Check 5: Exact versioned NSIS artifact selection
    # Look for patterns like "Mori Lens_${VERSION}_x64-setup.exe" or similar
    # This is a softer check - we just verify they're not using "head -1" or "Select-Object -First 1"
    # on arbitrary .exe files in the verify steps
    verify_steps = re.findall(
        r"name:\s*Verify (?:NSIS )?installer.*?(?=\n\s*-\s*name:|\Z)",
        content,
        re.DOTALL | re.IGNORECASE,
    )

    for step in verify_steps:
        # Check if this is a detailed/structural verify step (not just existence check)
        if "verify-nsis.ps1" in step:
            # If it uses Select-Object -First 1 or head -1 on *.exe, that's a violation
            if re.search(r"Select-Object\s+-First\s+1.*?\.exe", step, re.IGNORECASE):
                errors.append(
                    f"{workflow_path.name}: NSIS verification must use exact versioned "
                    f"artifact, not Select-Object -First 1 on *.exe"
                )
            if re.search(r"find.*?\.exe.*?\|\s*head\s+-1", step, re.IGNORECASE):
                errors.append(
                    f"{workflow_path.name}: NSIS verification must use exact versioned "
                    f"artifact, not find | head -1 on *.exe"
                )

    return errors


def main():
    """Check all workflow files and report results."""
    repo_root = Path(__file__).parent.parent
    workflows_dir = repo_root / ".github" / "workflows"

    workflows_to_check = [
        workflows_dir / "release.yml",
        workflows_dir / "package-smoke.yml",
    ]

    all_errors = []

    for workflow_path in workflows_to_check:
        errors = check_workflow_contract(workflow_path)
        all_errors.extend(errors)

    if all_errors:
        print("Workflow contract violations found:")
        for error in all_errors:
            print(f"  - {error}")
        sys.exit(1)
    else:
        print("Workflow contract: PASS")
        print(f"  Checked: {', '.join(w.name for w in workflows_to_check)}")
        print("  - Tauri builds scoped to apps/lens: OK")
        print("  - NSIS verifier receives payload path: OK")
        print("  - NSIS verifier receives expected version: OK")
        print("  - Exact versioned artifact selection: OK")
        sys.exit(0)


if __name__ == "__main__":
    main()
