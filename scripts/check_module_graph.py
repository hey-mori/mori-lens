#!/usr/bin/env python3
"""Check frontend module graph reachability.

Starting from HTML entrypoints, discover all loaded scripts and follow
their imports to build a reachable file set.

This ensures that:
- All JS modules under web/js/ are actually used
- No dead code exists in production modules
- Import cycles are detected
- Missing imports are caught
"""

import re
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple


def find_html_entrypoints(web_dir: Path) -> List[Path]:
    """Find all HTML entrypoints in web directory."""
    return list(web_dir.glob("*.html"))


def find_scripts_in_html(html_path: Path) -> List[str]:
    """Find all script src attributes in an HTML file.

    Returns list of relative paths to JS files.
    """
    try:
        content = html_path.read_text(encoding="utf-8")
    except Exception:
        return []

    # Match <script src="..."> or <script type="module" src="...">
    pattern = r'<script[^>]*\ssrc=["\']([^"\']+)["\'][^>]*>'
    matches = re.findall(pattern, content)

    return matches


def find_imports_in_js(js_path: Path, web_dir: Path) -> List[str]:
    """Find all import statements in a JS file.

    Returns list of resolved paths relative to web_dir.
    """
    try:
        content = js_path.read_text(encoding="utf-8")
    except Exception:
        return []

    imports = []

    # Match: import ... from "./x.js"
    # Match: import "./x.js"
    # Match: import("./x.js")  (dynamic import)
    # Match: export ... from "./x.js"
    patterns = [
        r'import\s+.*?\s+from\s+["\']([^"\']+)["\']',
        r'import\s+["\']([^"\']+)["\']',
        r'import\(\s*["\']([^"\']+)["\']\s*\)',
        r'export\s+.*?\s+from\s+["\']([^"\']+)["\']',
    ]

    for pattern in patterns:
        for match in re.finditer(pattern, content):
            import_path = match.group(1)
            # Resolve relative to the JS file's directory
            js_dir = js_path.parent
            resolved = (js_dir / import_path).resolve()
            try:
                rel_path = resolved.relative_to(web_dir.resolve())
                imports.append(str(rel_path))
            except ValueError:
                # Import points outside web_dir
                pass

    return imports


def build_module_graph(web_dir: Path) -> Tuple[Dict[str, Set[str]], Set[str], List[str]]:
    """Build module dependency graph starting from HTML entrypoints.

    Returns:
        - graph: dict mapping file -> set of imported files
        - reachable: set of all reachable files
        - errors: list of error messages
    """
    graph: Dict[str, Set[str]] = {}
    reachable: Set[str] = set()
    errors: List[str] = []

    # Find all HTML entrypoints
    entrypoints = find_html_entrypoints(web_dir)

    # BFS to discover all reachable modules
    queue: List[str] = []

    # Start with scripts referenced in HTML
    for html_path in entrypoints:
        html_rel = str(html_path.relative_to(web_dir))
        scripts = find_scripts_in_html(html_path)

        for script in scripts:
            script_path = web_dir / script
            if not script_path.exists():
                errors.append(f"{html_rel}: script not found: {script}")
                continue

            script_rel = str(script_path.relative_to(web_dir))
            if script_rel not in reachable:
                reachable.add(script_rel)
                queue.append(script_rel)

    # Follow imports
    while queue:
        current = queue.pop(0)
        current_path = web_dir / current

        if current not in graph:
            graph[current] = set()

        imports = find_imports_in_js(current_path, web_dir)

        for imp in imports:
            graph[current].add(imp)

            imp_path = web_dir / imp
            if not imp_path.exists():
                errors.append(f"{current}: import not found: {imp}")
                continue

            if imp not in reachable:
                reachable.add(imp)
                queue.append(imp)

    return graph, reachable, errors


def find_orphan_js_files(web_dir: Path, reachable: Set[str]) -> List[str]:
    """Find JS files under web/js/ that are not reachable."""
    js_dir = web_dir / "js"
    if not js_dir.exists():
        return []

    # Files that are intentionally standalone (not imported by other modules)
    allowed_orphans = {
        "js/i18n.test.js",  # Test file, run directly with node
        "js/i18n-key-checker.js",  # Standalone integrity checker tool
        "js\\i18n.test.js",  # Windows path separator
        "js\\i18n-key-checker.js",  # Windows path separator
        "js/test-infrastructure.js",  # Test infrastructure, imported by tests only
        "js\\test-infrastructure.js",  # Windows path separator
    }

    orphans = []
    for js_file in js_dir.glob("*.js"):
        js_rel = str(js_file.relative_to(web_dir))
        # Skip Vitest test files (they are run by Vitest, not imported)
        if js_rel.endswith(".vitest.test.js") or js_rel.endswith("\\*.vitest.test.js"):
            continue
        if js_rel not in reachable and js_rel not in allowed_orphans:
            orphans.append(js_rel)

    return orphans


def detect_cycles(graph: Dict[str, Set[str]]) -> List[List[str]]:
    """Detect import cycles in the module graph.

    Returns list of cycles (each cycle is a list of file paths).
    """
    cycles = []
    visited: Set[str] = set()
    path: List[str] = []

    def dfs(node: str):
        if node in path:
            # Found a cycle
            cycle_start = path.index(node)
            cycles.append(path[cycle_start:] + [node])
            return

        if node in visited:
            return

        visited.add(node)
        path.append(node)

        for neighbor in graph.get(node, set()):
            dfs(neighbor)

        path.pop()

    for node in graph:
        if node not in visited:
            dfs(node)

    return cycles


def check_module_graph(root: Path) -> List[str]:
    """Check module graph reachability and integrity.

    Returns list of error messages (empty if all good).
    """
    web_dir = root / "apps" / "lens" / "web"
    if not web_dir.exists():
        return [f"web directory not found: {web_dir}"]

    errors = []

    # Build module graph
    graph, reachable, build_errors = build_module_graph(web_dir)
    errors.extend(build_errors)

    # Find orphan JS files
    orphans = find_orphan_js_files(web_dir, reachable)
    for orphan in orphans:
        errors.append(f"orphan JS file (not imported): {orphan}")

    # Detect cycles
    cycles = detect_cycles(graph)
    for cycle in cycles:
        cycle_str = " -> ".join(cycle)
        errors.append(f"import cycle detected: {cycle_str}")

    return errors


def main():
    root = Path(__file__).parent.parent
    errors = check_module_graph(root)

    if errors:
        print("Module graph check failed:\n")
        for error in errors:
            print(f"  {error}\n")
        sys.exit(1)
    else:
        web_dir = root / "apps" / "lens" / "web"
        _, reachable, _ = build_module_graph(web_dir)
        print("Module graph check passed.")
        print(f"  Reachable files: {len(reachable)}")
        for f in sorted(reachable):
            print(f"    - {f}")
        sys.exit(0)


if __name__ == "__main__":
    main()
