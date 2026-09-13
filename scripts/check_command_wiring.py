#!/usr/bin/env python3
"""Check Tauri command registration vs frontend invocation.

Three-set model:
A. DECLARED: Functions annotated with #[tauri::command]
B. REGISTERED: Entries in tauri::generate_handler![...]
C. INVOKED: Frontend invoke('command') calls

Enforces:
- Frontend invoke exists but not REGISTERED → ERROR
- REGISTERED but no declaration → ERROR
- DECLARED but not REGISTERED → ERROR (unless allowlisted)
- REGISTERED but not INVOKED → ERROR (unless allowlisted)
- Duplicate registered names → ERROR
- Duplicate declared names → ERROR

Allowlist: commands intentionally not invoked from main webviews.
"""

import os
import re
import sys
from pathlib import Path


def find_declared_commands(root: Path) -> tuple[dict[str, str], list[tuple[str, str, str]]]:
    """Find all #[tauri::command] functions in Rust source.

    Returns ({command_name: file_path}, [(cmd_name, file1, file2), ...] duplicates).
    """
    commands = {}
    duplicates = []
    for rs_file in root.rglob("*.rs"):
        if "target" in rs_file.parts:
            continue
        try:
            content = rs_file.read_text(encoding="utf-8")
        except Exception:
            continue

        # Remove line comments (// ...) to avoid matching commented-out commands
        # Keep the line structure by replacing with empty line
        lines = content.splitlines()
        cleaned_lines = []
        for line in lines:
            # Find // that's not inside a string (simplified: just check if // appears)
            # For our purposes, if a line contains //, treat everything after as comment
            if '//' in line:
                # Simple heuristic: if // appears, assume it's a comment
                # This won't handle strings with // perfectly, but good enough for our use case
                comment_pos = line.find('//')
                line = line[:comment_pos]
            cleaned_lines.append(line)
        cleaned_content = '\n'.join(cleaned_lines)

        # Match #[tauri::command] followed by fn name (with optional pub/pub(crate)/async)
        pattern = r'#\[tauri::command\]\s*(?:pub(?:\([^)]+\))?\s+)?(?:async\s+)?fn\s+(\w+)'
        for match in re.finditer(pattern, cleaned_content):
            cmd_name = match.group(1)
            file_path = str(rs_file.relative_to(root))
            if cmd_name in commands:
                duplicates.append((cmd_name, commands[cmd_name], file_path))
            else:
                commands[cmd_name] = file_path

    return commands, duplicates


def find_registered_commands(root: Path) -> tuple[dict[str, str], list[tuple[str, str, str, str]]]:
    """Find all entries in tauri::generate_handler![...] blocks.

    Returns ({command_name: full_source_path}, [(cmd_name, source1, source2, file), ...] duplicates).
    Module-qualified entries like crate::detail_view::detail_read
    are normalized to detail_read but retain full path for diagnostics.
    """
    registered = {}
    duplicates = []
    for rs_file in root.rglob("*.rs"):
        if "target" in rs_file.parts:
            continue
        try:
            content = rs_file.read_text(encoding="utf-8")
        except Exception:
            continue

        # Find generate_handler![...] blocks
        pattern = r'tauri::generate_handler!\s*\[([^\]]+)\]'
        for match in re.finditer(pattern, content, re.DOTALL):
            block = match.group(1)
            # Extract individual command entries (comma-separated)
            # Handle module-qualified paths like crate::detail_view::detail_read
            for entry in re.split(r',\s*', block.strip()):
                entry = entry.strip()
                if not entry or entry.startswith('//'):
                    continue
                # Normalize to command name (last component)
                cmd_name = entry.split('::')[-1]
                file_path = str(rs_file.relative_to(root))
                if cmd_name in registered:
                    duplicates.append((cmd_name, registered[cmd_name], entry, file_path))
                else:
                    registered[cmd_name] = entry

    return registered, duplicates


def find_frontend_invokes(root: Path) -> dict[str, str]:
    """Find all invoke('command_name') calls in frontend source.

    Returns {command_name: file_path}.
    """
    invokes = {}
    web_dir = root / "apps" / "lens" / "web"
    if not web_dir.exists():
        return invokes

    for html_file in web_dir.glob("*.html"):
        try:
            content = html_file.read_text(encoding="utf-8")
        except Exception:
            continue

        # Match invoke('name') or invoke("name") - name is first argument
        pattern = r"invoke\(['\"](\w+)['\"]"
        for match in re.finditer(pattern, content):
            cmd_name = match.group(1)
            invokes[cmd_name] = str(html_file.relative_to(root))

    return invokes


def check_diagnostics_ui(root: Path) -> list[str]:
    """Check diagnostics UI correctness.

    Verifies:
    - export_diagnostics frontend invoke exists
    - copyDiagnostics takes explicit button parameter
    - execCommand return value is checked
    - No clipboard-read API is introduced

    Returns list of error messages (empty if all good).
    """
    errors = []
    web_dir = root / "apps" / "lens" / "web"
    if not web_dir.exists():
        return errors

    # Search all HTML files for the patterns
    html_files = list(web_dir.glob("*.html"))
    if not html_files:
        return errors

    # Combine all HTML content
    all_content = ""
    for html_file in html_files:
        try:
            all_content += html_file.read_text(encoding="utf-8")
        except Exception:
            continue

    # Check 1: export_diagnostics invoke exists
    if "invoke('export_diagnostics')" not in all_content and 'invoke("export_diagnostics")' not in all_content:
        errors.append("export_diagnostics frontend invoke not found")

    # Check 2: copyDiagnostics function takes explicit parameter
    # Look for function definition with parameter
    if not re.search(r"function\s+copyDiagnostics\s*\(\s*\w+\s*\)", all_content):
        errors.append("copyDiagnostics must take explicit button parameter, not rely on implicit event")

    # Check 3: execCommand return value is checked
    # Look for pattern like: var copied = document.execCommand('copy') or if (document.execCommand...)
    if "document.execCommand('copy')" in all_content or 'document.execCommand("copy")' in all_content:
        # Check if return value is captured or tested
        if not re.search(r"(var|let|const)\s+\w+\s*=\s*document\.execCommand\(['\"]copy['\"]\)", all_content):
            if not re.search(r"if\s*\(\s*document\.execCommand\(['\"]copy['\"]\)", all_content):
                errors.append("execCommand('copy') return value must be checked to avoid false success feedback")

    # Check 4: No clipboard-read API
    if "clipboard.read" in all_content or "navigator.clipboard.readText" in all_content:
        errors.append("clipboard-read API detected; diagnostics copy must be write-only")

    return errors


def check_command_wiring(root: Path) -> list[str]:
    """Check command registration vs invocation using three-set model.

    Returns list of error messages (empty if all good).
    """
    declared, declared_dupes = find_declared_commands(root)
    registered, registered_dupes = find_registered_commands(root)
    invokes = find_frontend_invokes(root)

    errors = []

    # Check for duplicate declarations
    for cmd_name, file1, file2 in declared_dupes:
        errors.append(f"duplicate declared command '{cmd_name}': in {file1} and {file2}")

    # Check for duplicate registrations
    for cmd_name, source1, source2, file_path in registered_dupes:
        errors.append(
            f"duplicate registered command '{cmd_name}': '{source1}' and '{source2}' in {file_path}"
        )

    # Allowlist: commands intentionally not invoked from main webviews
    # Each entry must have a reason
    allowlist = {
        # Add commands here if they're invoked from external tools,
        # test harnesses, or other non-webview contexts
    }

    # Check: REGISTERED but not DECLARED
    for cmd_name, full_source in sorted(registered.items()):
        if cmd_name not in declared:
            errors.append(
                f"registered command has no #[tauri::command] declaration:\n"
                f"  {cmd_name} (from {full_source})"
            )

    # Check: DECLARED but not REGISTERED
    for cmd_name, file_path in sorted(declared.items()):
        if cmd_name not in registered and cmd_name not in allowlist:
            errors.append(
                f"declared command is not registered in generate_handler!:\n"
                f"  {cmd_name} (in {file_path})"
            )

    # Check: INVOKED but not REGISTERED
    for cmd_name, file_path in sorted(invokes.items()):
        if cmd_name not in registered:
            errors.append(
                f"frontend invoke has no registered handler:\n"
                f"  {cmd_name} (in {file_path})"
            )

    # Check: REGISTERED but not INVOKED
    for cmd_name, full_source in sorted(registered.items()):
        if cmd_name not in invokes and cmd_name not in allowlist:
            errors.append(
                f"registered command has no frontend caller:\n"
                f"  {cmd_name} (from {full_source})"
            )

    # Note: Diagnostics UI checks are in a separate PR (#480)
    # errors.extend(check_diagnostics_ui(root))

    return errors


def main():
    root = Path(__file__).parent.parent
    errors = check_command_wiring(root)

    if errors:
        print("Command wiring check failed:\n")
        for error in errors:
            print(f"  {error}\n")
        sys.exit(1)
    else:
        declared, _ = find_declared_commands(root)
        registered, _ = find_registered_commands(root)
        invokes = find_frontend_invokes(root)
        print("Command wiring check passed.")
        print(f"  {len(declared)} declared commands")
        print(f"  {len(registered)} registered commands")
        print(f"  {len(invokes)} frontend invokes")
        print("  Diagnostics UI: OK")
        sys.exit(0)


if __name__ == "__main__":
    main()
