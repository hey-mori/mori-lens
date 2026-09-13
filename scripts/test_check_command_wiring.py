#!/usr/bin/env python3
"""Regression tests for command wiring checker.

Tests the three-set model:
A. DECLARED: Functions annotated with #[tauri::command]
B. REGISTERED: Entries in tauri::generate_handler![...]
C. INVOKED: Frontend invoke('command') calls
"""

import sys
import tempfile
from pathlib import Path

# Ensure we import from the same directory as this test file
sys.path.insert(0, str(Path(__file__).parent))

from check_command_wiring import (
    check_command_wiring,
    find_declared_commands,
    find_registered_commands,
    find_frontend_invokes,
)


def test_declared_registered_invoked():
    """PASS: declared + registered + invoked."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)

        # Create Rust source with declared command
        rs_dir = root / "src"
        rs_dir.mkdir()
        (rs_dir / "main.rs").write_text(
            """
#[tauri::command]
fn foo() {}

tauri::generate_handler![foo]
"""
        )

        # Create frontend with invoke
        web_dir = root / "apps" / "lens" / "web"
        web_dir.mkdir(parents=True)
        (web_dir / "index.html").write_text("invoke('foo')")

        errors = check_command_wiring(root)
        assert errors == [], f"Expected no errors, got: {errors}"


def test_declared_invoked_not_registered():
    """FAIL: declared + invoked but NOT registered."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)

        # Create Rust source with declared command but not registered
        rs_dir = root / "src"
        rs_dir.mkdir()
        (rs_dir / "main.rs").write_text(
            """
#[tauri::command]
fn foo() {}

tauri::generate_handler![]
"""
        )

        # Create frontend with invoke
        web_dir = root / "apps" / "lens" / "web"
        web_dir.mkdir(parents=True)
        (web_dir / "index.html").write_text("invoke('foo')")

        errors = check_command_wiring(root)
        assert len(errors) > 0
        assert any("not registered" in e or "has no registered handler" in e for e in errors)


def test_registered_invoked_no_declaration():
    """FAIL: registered + invoked but no declaration."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)

        # Create Rust source with registered but no declaration
        rs_dir = root / "src"
        rs_dir.mkdir()
        (rs_dir / "main.rs").write_text(
            """
// No #[tauri::command] fn foo()

tauri::generate_handler![foo]
"""
        )

        # Create frontend with invoke
        web_dir = root / "apps" / "lens" / "web"
        web_dir.mkdir(parents=True)
        (web_dir / "index.html").write_text("invoke('foo')")

        errors = check_command_wiring(root)
        assert len(errors) > 0
        assert any("no #[tauri::command] declaration" in e for e in errors)


def test_declared_registered_not_invoked():
    """FAIL: declared + registered but no frontend caller."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)

        # Create Rust source with declared and registered
        rs_dir = root / "src"
        rs_dir.mkdir()
        (rs_dir / "main.rs").write_text(
            """
#[tauri::command]
fn foo() {}

tauri::generate_handler![foo]
"""
        )

        # Create frontend without invoke
        web_dir = root / "apps" / "lens" / "web"
        web_dir.mkdir(parents=True)
        (web_dir / "index.html").write_text("// no invoke")

        errors = check_command_wiring(root)
        assert len(errors) > 0
        assert any("no frontend caller" in e for e in errors)


def test_module_qualified_handler():
    """PASS: module-qualified generate_handler entry."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)

        # Create Rust source with module-qualified registration
        rs_dir = root / "src"
        rs_dir.mkdir()
        (rs_dir / "main.rs").write_text(
            """
#[tauri::command]
fn detail_read() {}

tauri::generate_handler![crate::detail_view::detail_read]
"""
        )

        # Create frontend with invoke
        web_dir = root / "apps" / "lens" / "web"
        web_dir.mkdir(parents=True)
        (web_dir / "index.html").write_text("invoke('detail_read')")

        errors = check_command_wiring(root)
        assert errors == [], f"Expected no errors, got: {errors}"

        # Verify normalization
        registered, _ = find_registered_commands(root)
        assert "detail_read" in registered
        assert registered["detail_read"] == "crate::detail_view::detail_read"


def test_duplicate_registration():
    """FAIL: duplicate command registration."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)

        # Create Rust source with duplicate registration
        rs_dir = root / "src"
        rs_dir.mkdir()
        (rs_dir / "main.rs").write_text(
            """
#[tauri::command]
fn foo() {}

tauri::generate_handler![foo, foo]
"""
        )

        # Create frontend with invoke
        web_dir = root / "apps" / "lens" / "web"
        web_dir.mkdir(parents=True)
        (web_dir / "index.html").write_text("invoke('foo')")

        errors = check_command_wiring(root)
        assert len(errors) > 0
        assert any("duplicate registered" in e for e in errors)


def test_frontend_unknown_command():
    """FAIL: frontend invoke with no registered handler."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)

        # Create Rust source without the command
        rs_dir = root / "src"
        rs_dir.mkdir()
        (rs_dir / "main.rs").write_text(
            """
tauri::generate_handler![]
"""
        )

        # Create frontend with invoke
        web_dir = root / "apps" / "lens" / "web"
        web_dir.mkdir(parents=True)
        (web_dir / "index.html").write_text("invoke('foo')")

        errors = check_command_wiring(root)
        assert len(errors) > 0
        assert any("no registered handler" in e for e in errors)


if __name__ == "__main__":
    test_declared_registered_invoked()
    print("PASS: test_declared_registered_invoked")

    test_declared_invoked_not_registered()
    print("PASS: test_declared_invoked_not_registered")

    test_registered_invoked_no_declaration()
    print("PASS: test_registered_invoked_no_declaration")

    test_declared_registered_not_invoked()
    print("PASS: test_declared_registered_not_invoked")

    test_module_qualified_handler()
    print("PASS: test_module_qualified_handler")

    test_duplicate_registration()
    print("PASS: test_duplicate_registration")

    test_frontend_unknown_command()
    print("PASS: test_frontend_unknown_command")

    print("\nAll tests passed!")
