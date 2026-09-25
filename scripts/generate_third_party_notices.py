#!/usr/bin/env python3
"""
Generate accurate THIRD-PARTY-NOTICES.md from cargo metadata.

This script:
- Uses cargo metadata to get complete dependency information
- Detects first-party packages mechanically by manifest_path (inside repo)
- Preserves exact SPDX license expressions
- Distinguishes repository inventory from Windows distribution inventory
- Verifies JavaScript dev-only claim from package.json
- Does not make legal conclusions from naive substring matching
"""

import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

# Force UTF-8 output for Windows
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

REPO_ROOT = Path(__file__).parent.parent.absolute()


def run_cargo_metadata(manifest_path):
    """Run cargo metadata and return parsed JSON."""
    cmd = [
        "cargo", "metadata",
        "--format-version=1",
        "--manifest-path", str(manifest_path),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
    if result.returncode != 0:
        print(f"Error running cargo metadata: {result.stderr}", file=sys.stderr)
        sys.exit(1)
    return json.loads(result.stdout)


def is_first_party(package, repo_root):
    """Check if a package is first-party by checking if its manifest is inside the repo."""
    manifest_path = Path(package.get("manifest_path", ""))
    try:
        manifest_path.relative_to(repo_root)
        return True
    except ValueError:
        return False


def check_js_dev_only():
    """Verify JavaScript dependencies are dev-only from package.json."""
    package_json_path = REPO_ROOT / "apps" / "lens" / "package.json"
    if not package_json_path.exists():
        return False, "package.json not found"

    with open(package_json_path, encoding="utf-8") as f:
        pkg = json.load(f)

    has_deps = "dependencies" in pkg and len(pkg["dependencies"]) > 0
    has_dev_deps = "devDependencies" in pkg and len(pkg["devDependencies"]) > 0

    if has_deps:
        return False, f"Found runtime dependencies: {list(pkg['dependencies'].keys())}"
    elif has_dev_deps:
        return True, f"Only devDependencies: {list(pkg['devDependencies'].keys())}"
    else:
        return True, "No JavaScript dependencies"


def get_windows_target_deps(metadata, repo_root):
    """Filter dependencies for Windows target."""
    # This is a simplification - in reality we'd need to evaluate target cfg expressions
    # For now, we include all deps and note that some are platform-conditional
    first_party = []
    third_party = []

    for package in metadata.get("packages", []):
        if is_first_party(package, repo_root):
            first_party.append(package)
        else:
            third_party.append(package)

    return first_party, third_party


def main():
    print("=" * 70)
    print("Mori Lens Third-Party Notice Generator")
    print("=" * 70)

    # Get workspace metadata (excludes apps/lens)
    print("\n1. Loading workspace metadata...")
    workspace_metadata = run_cargo_metadata(REPO_ROOT / "Cargo.toml")
    workspace_pkgs = workspace_metadata.get("packages", [])
    print(f"   Found {len(workspace_pkgs)} workspace packages")

    # Get lens app metadata (separate workspace)
    print("\n2. Loading lens app metadata...")
    lens_metadata = run_cargo_metadata(REPO_ROOT / "apps" / "lens" / "Cargo.toml")
    lens_pkgs = lens_metadata.get("packages", [])
    print(f"   Found {len(lens_pkgs)} lens app packages")

    # Combine all packages
    all_packages = {pkg["name"]: pkg for pkg in workspace_pkgs}
    for pkg in lens_pkgs:
        if pkg["name"] not in all_packages:
            all_packages[pkg["name"]] = pkg

    print(f"\n3. Total unique packages: {len(all_packages)}")

    # Separate first-party from third-party
    first_party = []
    third_party = []

    for pkg in all_packages.values():
        if is_first_party(pkg, REPO_ROOT):
            first_party.append(pkg)
        else:
            third_party.append(pkg)

    print(f"   First-party (Mori Lens): {len(first_party)}")
    print(f"   Third-party: {len(third_party)}")

    # Verify JavaScript dev-only claim
    print("\n4. Verifying JavaScript dependency claim...")
    js_dev_only, js_msg = check_js_dev_only()
    print(f"   {js_msg}")

    # Group third-party by exact license expression
    by_license = defaultdict(list)
    for pkg in third_party:
        license_expr = pkg.get("license") or "NO-License"
        by_license[license_expr].append(pkg)

    print(f"\n5. Unique license expressions: {len(by_license)}")

    # Generate THIRD-PARTY-NOTICES.md
    output_path = REPO_ROOT / "THIRD-PARTY-NOTICES.md"
    print(f"\n6. Generating {output_path}...")

    with open(output_path, "w", encoding="utf-8") as f:
        f.write("# Third-Party Notices\n\n")
        f.write("This document lists the third-party dependencies used by Mori Lens.\n\n")
        f.write("Generated by `scripts/generate_third_party_notices.py` from `cargo metadata`.\n\n")

        f.write("## Summary\n\n")
        f.write(f"- **Repository dependency inventory**: {len(all_packages)} packages\n")
        f.write(f"- **First-party Mori Lens crates**: {len(first_party)} (not listed as third-party obligations)\n")
        f.write(f"- **Third-party dependencies**: {len(third_party)}\n")
        f.write(f"- **Unique license expressions**: {len(by_license)}\n\n")

        f.write("### Target Platform Note\n\n")
        f.write("This is the **repository dependency inventory** — it includes dependencies for all target platforms ")
        f.write("(Windows, macOS, Linux). The actual Windows v0.1.0 distribution includes only Windows-compatible ")
        f.write("dependencies. Platform-conditional dependencies (e.g., macOS-only crates) are not shipped in the ")
        f.write("Windows binary but are listed here for completeness.\n\n")

        f.write("### JavaScript Dependencies\n\n")
        if js_dev_only:
            f.write(f"JavaScript dependencies are **development-only** (not distributed). {js_msg}. ")
            f.write("They are listed as `devDependencies` in `apps/lens/package.json` and are **not included in the ")
            f.write("distributed application**. They do not create distribution obligations.\n\n")
        else:
            f.write(f"**Warning**: {js_msg}. Review for distribution obligations.\n\n")

        f.write("### First-Party Crates\n\n")
        f.write("The following are Mori Lens's own crates and are not third-party obligations:\n\n")
        for pkg in sorted(first_party, key=lambda p: p["name"]):
            f.write(f"- `{pkg['name']}` {pkg['version']}\n")
        f.write("\n")

        f.write("## Third-Party Dependencies by License\n\n")
        f.write("License expressions are preserved exactly as reported by `cargo metadata`.\n\n")

        for lic in sorted(by_license.keys()):
            deps = by_license[lic]
            f.write(f"### {lic}\n\n")
            f.write(f"{len(deps)} packages:\n\n")
            for dep in sorted(deps, key=lambda d: d["name"]):
                f.write(f"- `{dep['name']}` {dep['version']}\n")
            f.write("\n")

        f.write("## Full Third-Party Dependency List\n\n")
        f.write("| Package | Version | License |\n")
        f.write("|---------|---------|----------|\n")
        for pkg in sorted(third_party, key=lambda d: d["name"]):
            license_expr = pkg.get("license") or "NO-License"
            f.write(f"| `{pkg['name']}` | {pkg['version']} | {license_expr} |\n")

    print(f"\n✓ Generated {output_path}")
    print(f"\n" + "=" * 70)
    print("Summary:")
    print(f"  First-party: {len(first_party)}")
    print(f"  Third-party: {len(third_party)}")
    print(f"  Unique licenses: {len(by_license)}")
    print(f"  JavaScript: {'dev-only' if js_dev_only else 'HAS RUNTIME DEPS'}")
    print("=" * 70)
    print("\nNote: This generator uses cargo metadata to detect first-party packages")
    print("mechanically by manifest_path, not by hardcoded name lists.")


if __name__ == "__main__":
    main()
