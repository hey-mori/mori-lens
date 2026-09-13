#!/usr/bin/env python3
"""Check Tauri command registration vs frontend invocation.

Detects:
- Registered commands with no frontend caller (dead commands)
- Frontend invokes with no registered handler (broken wiring)

Allowlist: commands intentionally invoked outside main webviews.
"""

import os
import re
import sys
from pathlib import Path


def find_tauri_commands(root: Path) -> dict[str, str]:
    """Find all #[tauri::command] functions in Rust source.

    Returns {command_name: file_path}.
    """
    commands = {}
    for rs_file in root.rglob("*.rs"):
        if "target" in rs_file.parts:
            continue
        try:
            content = rs_file.read_text(encoding="utf-8")
        except Exception:
            continue

        # Match #[tauri::command] followed by fn name (with optional pub/pub(crate)/async)
        pattern = r'#\[tauri::command\]\s*(?:pub(?:\([^)]+\))?\s+)?(?:async\s+)?fn\s+(\w+)'
        for match in re.finditer(pattern, content):
            cmd_name = match.group(1)
            commands[cmd_name] = str(rs_file.relative_to(root))

    return commands


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


def check_command_wiring(root: Path) -> list[str]:
    """Check command registration vs invocation.

    Returns list of error messages (empty if all good).
    """
    commands = find_tauri_commands(root)
    invokes = find_frontend_invokes(root)

    errors = []

    # Allowlist: commands intentionally not invoked from main webviews
    # Each entry must have a reason
    allowlist = {
        # Add commands here if they're invoked from external tools,
        # test harnesses, or other non-webview contexts
    }

    # Check for dead commands (registered but never invoked)
    for cmd_name, file_path in sorted(commands.items()):
        if cmd_name not in invokes and cmd_name not in allowlist:
            errors.append(
                f"registered Tauri command has no frontend caller:\n"
                f"  {cmd_name} (in {file_path})"
            )

    # Check for broken wiring (invoked but not registered)
    for cmd_name, file_path in sorted(invokes.items()):
        if cmd_name not in commands:
            errors.append(
                f"frontend invoke has no registered handler:\n"
                f"  {cmd_name} (in {file_path})"
            )

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
        print("Command wiring check passed.")
        print(f"  {len(find_tauri_commands(root))} registered commands")
        print(f"  {len(find_frontend_invokes(root))} frontend invokes")
        sys.exit(0)


if __name__ == "__main__":
    main()
