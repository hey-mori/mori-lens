#!/usr/bin/env python3
"""
Regression tests for the DOM trust boundary checker.

Verifies that check_trust_boundary.py correctly identifies violations
and accepts safe patterns. Uses in-memory test fixtures.
"""

import sys
import tempfile
from pathlib import Path

# Add scripts directory to path so we can import the checker
sys.path.insert(0, str(Path(__file__).parent))
import check_trust_boundary


def test_unsafe_template_literal_detected():
    """Unsafe template literal interpolation should be detected."""
    code = 'el.innerHTML = `<p>${userInput}</p>`;'
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 1
        assert violations[0]['pattern'] == 'template-literal'
        assert violations[0]['variable'] == 'userInput'
    Path(f.name).unlink()


def test_safe_template_literal_accepted():
    """Template literal with escapeHtml should be accepted."""
    code = 'el.innerHTML = `<p>${escapeHtml(String(userInput))}</p>`;'
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 0
    Path(f.name).unlink()


def test_unsafe_concatenation_detected():
    """Unsafe string concatenation with error pattern should be detected."""
    code = "el.innerHTML = '<p>Error: ' + userInput + '</p>';"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 1
    Path(f.name).unlink()


def test_unsafe_error_concat_detected():
    """Unsafe error message concatenation should be detected."""
    code = "el.innerHTML = '<p class=\"error\">Error: ' + e + '</p>';"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 1
        assert violations[0]['pattern'] == 'error-concat'
        assert violations[0]['variable'] == 'e'
    Path(f.name).unlink()


def test_safe_error_concat_accepted():
    """Error message with escapeHtml should be accepted."""
    code = "el.innerHTML = '<p class=\"error\">Error: ' + escapeHtml(String(e)) + '</p>';"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 0
    Path(f.name).unlink()


def test_unsafe_model_output_detected():
    """Raw model output in innerHTML should be detected."""
    code = "el.innerHTML = '<strong>' + result.model + '</strong>';"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 1
        assert violations[0]['pattern'] == 'model-output'
    Path(f.name).unlink()


def test_safe_model_output_accepted():
    """Model output with escapeHtml should be accepted."""
    code = "el.innerHTML = '<strong>' + escapeHtml(String(result.model)) + '</strong>';"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 0
    Path(f.name).unlink()


def test_text_content_accepted():
    """textContent is safe and should be accepted."""
    code = "el.textContent = userInput;"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 0
    Path(f.name).unlink()


def test_numeric_variables_skipped():
    """Numeric variables like count, i, index, length should be skipped."""
    code = "el.innerHTML = `<p>Found ${count} items</p>`;"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 0
    Path(f.name).unlink()


def test_comments_skipped():
    """Comments should not be checked."""
    code = "// el.innerHTML = `<p>${userInput}</p>`;"
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(code)
        f.flush()
        violations = check_trust_boundary.check_file(Path(f.name))
        assert len(violations) == 0
    Path(f.name).unlink()


if __name__ == '__main__':
    tests = [
        test_unsafe_template_literal_detected,
        test_safe_template_literal_accepted,
        test_unsafe_concatenation_detected,
        test_unsafe_error_concat_detected,
        test_safe_error_concat_accepted,
        test_unsafe_model_output_detected,
        test_safe_model_output_accepted,
        test_text_content_accepted,
        test_numeric_variables_skipped,
        test_comments_skipped,
    ]

    failed = 0
    for test in tests:
        try:
            test()
            print(f"PASS: {test.__name__}")
        except AssertionError as e:
            print(f"FAIL: {test.__name__}: {e}")
            failed += 1
        except Exception as e:
            print(f"FAIL: {test.__name__}: {e}")
            failed += 1

    if failed:
        print(f"\n{failed} test(s) failed")
        sys.exit(1)
    else:
        print(f"\nAll {len(tests)} tests passed")
        sys.exit(0)
