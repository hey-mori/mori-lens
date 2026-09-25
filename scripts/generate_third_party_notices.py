#!/usr/bin/env python3
"""
Generate accurate THIRD-PARTY-NOTICES.md from actual dependency manifests.
"""

import json
import sys
from collections import defaultdict
from pathlib import Path

# Force UTF-8 output for Windows
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

def load_deps(json_path):
    """Load dependencies from cargo-license JSON output."""
    with open(json_path, encoding="utf-8") as f:
        return json.load(f)

def classify_dep(dep):
    """Classify a dependency by type and license."""
    name = dep.get("name", "")
    license_str = dep.get("license") or ""
    version = dep.get("version", "")

    # Classify license type
    license_type = "UNKNOWN"
    if "MIT" in license_str or "Apache" in license_str or "BSD" in license_str:
        license_type = "PERMISSIVE"
    elif "MPL" in license_str:
        license_type = "WEAK-COPYLEFT"
    elif "GPL" in license_str or "AGPL" in license_str:
        license_type = "COPYLEFT"

    return {
        "name": name,
        "version": version,
        "license": license_str or "UNKNOWN",
        "license_type": license_type,
    }

def main():
    print("Loading workspace dependencies...")
    workspace_deps = load_deps("workspace-deps.json")

    print("Loading lens app dependencies...")
    lens_deps = load_deps("apps/lens/lens-deps.json")

    # Combine and deduplicate
    all_deps = {}
    for dep in workspace_deps + lens_deps:
        key = f"{dep['name']}=={dep['version']}"
        if key not in all_deps:
            all_deps[key] = classify_dep(dep)

    print(f"\nTotal unique dependencies: {len(all_deps)}")

    # Group by license type
    by_type = defaultdict(list)
    for dep in all_deps.values():
        by_type[dep["license_type"]].append(dep)

    print("\nLicense distribution:")
    for ltype, deps in sorted(by_type.items()):
        print(f"  {ltype}: {len(deps)}")

    # Check for copyleft
    copyleft = by_type.get("COPYLEFT", [])
    if copyleft:
        print("\n⚠️  COPYLEFT DEPENDENCIES FOUND:")
        for dep in copyleft:
            print(f"  - {dep['name']} {dep['version']}: {dep['license']}")
    else:
        print("\n✓ No copyleft dependencies")

    # Check for weak-copyleft (MPL)
    weak_copyleft = by_type.get("WEAK-COPYLEFT", [])
    if weak_copyleft:
        print("\n⚠️  WEAK-COPYLEFT (MPL) DEPENDENCIES:")
        for dep in weak_copyleft:
            print(f"  - {dep['name']} {dep['version']}: {dep['license']}")

    # Group by license for notice generation
    by_license = defaultdict(list)
    for dep in all_deps.values():
        by_license[dep["license"]].append(dep)

    print("\nLicense groups:")
    for lic, deps in sorted(by_license.items(), key=lambda x: -len(x[1])):
        print(f"  {lic}: {len(deps)} packages")

    # Generate THIRD-PARTY-NOTICES.md
    output_path = Path("THIRD-PARTY-NOTICES.md")
    print(f"\nGenerating {output_path}...")

    with open(output_path, "w", encoding="utf-8") as f:
        f.write("# Third-Party Notices\n\n")
        f.write("This document lists the third-party dependencies used by Mori Lens.\n\n")

        f.write("## Summary\n\n")
        f.write(f"- **Total dependencies**: {len(all_deps)}\n")
        f.write(f"- **Permissive licenses**: {len(by_type.get('PERMISSIVE', []))}\n")
        f.write(f"- **Weak-copyleft (MPL)**: {len(by_type.get('WEAK-COPYLEFT', []))}\n")
        f.write(f"- **Copyleft (GPL/AGPL)**: {len(by_type.get('COPYLEFT', []))}\n\n")

        if copyleft:
            f.write("### ⚠️ Copyleft Dependencies\n\n")
            f.write("The following dependencies use copyleft licenses:\n\n")
            for dep in sorted(copyleft, key=lambda d: d["name"]):
                f.write(f"- `{dep['name']}` {dep['version']}: {dep['license']}\n")
            f.write("\n")

        if weak_copyleft:
            f.write("### Weak-Copyleft (MPL) Dependencies\n\n")
            f.write("The following dependencies use MPL or similar weak-copyleft licenses.\n")
            f.write("These are typically file-level copyleft and do not affect the larger work:\n\n")
            for dep in sorted(weak_copyleft, key=lambda d: d["name"]):
                f.write(f"- `{dep['name']}` {dep['version']}: {dep['license']}\n")
            f.write("\n")

        f.write("## Dependencies by License\n\n")
        for lic in sorted(by_license.keys()):
            deps = by_license[lic]
            f.write(f"### {lic}\n\n")
            f.write(f"{len(deps)} packages:\n\n")
            for dep in sorted(deps, key=lambda d: d["name"]):
                f.write(f"- `{dep['name']}` {dep['version']}\n")
            f.write("\n")

        f.write("## Full Dependency List\n\n")
        f.write("| Package | Version | License |\n")
        f.write("|---------|---------|----------|\n")
        for dep in sorted(all_deps.values(), key=lambda d: d["name"]):
            f.write(f"| `{dep['name']}` | {dep['version']} | {dep['license']} |\n")

    print(f"✓ Generated {output_path}")

if __name__ == "__main__":
    main()
