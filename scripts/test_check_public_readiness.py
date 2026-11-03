#!/usr/bin/env python3
"""
Regression tests for the licence check in check_public_readiness.py.

The repository may be in one of two consistent states, and the gate has to
accept both without choosing between them:

- no licence: no LICENSE file, the README says it is not licensed for use, and
  every Cargo manifest says UNLICENSED;
- a licence: a LICENSE file, the README names the licence, and every Cargo
  manifest carries that same SPDX expression.

Anything in between is a repository that tells a reader two different things
about what they may do with it, and must fail. The first version of the check
accepted only the first state, so adding any licence would have turned the
gate red.
"""

import sys
import tempfile
from pathlib import Path

# No bytecode: the gate scans the working tree for attribution patterns, and a
# cached copy of its own pattern list under scripts/__pycache__ would fail it.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import check_public_readiness as gate  # noqa: E402

ALL_RIGHTS = "## License\n\n**Not licensed for use.** The author holds the copyright.\n"


def tree(readme, licence_file, workspace_licence, app_licence):
    root = Path(tempfile.mkdtemp())
    (root / "README.md").write_text(readme, encoding="utf-8")
    if licence_file:
        (root / "LICENSE").write_text("licence text\n", encoding="utf-8")
    (root / "Cargo.toml").write_text(
        f'[workspace.package]\nlicense = "{workspace_licence}"\n', encoding="utf-8"
    )
    (root / "apps" / "lens").mkdir(parents=True)
    (root / "apps" / "lens" / "Cargo.toml").write_text(
        f'[package]\nname = "mori-lens"\nlicense = "{app_licence}"\n', encoding="utf-8"
    )
    return root


def verdict(root):
    gate.REPO_ROOT = root
    gate.EXIT_CODE = 0
    gate.check_license_state()
    return gate.EXIT_CODE


def test_no_licence_anywhere_passes():
    assert verdict(tree(ALL_RIGHTS, False, "UNLICENSED", "UNLICENSED")) == 0


def test_one_licence_everywhere_passes():
    readme = "## License\n\nLicensed under the MPL-2.0. See [LICENSE](LICENSE).\n"
    assert verdict(tree(readme, True, "MPL-2.0", "MPL-2.0")) == 0


def test_licence_file_with_an_all_rights_reserved_readme_fails():
    assert verdict(tree(ALL_RIGHTS, True, "MPL-2.0", "MPL-2.0")) == 1


def test_licence_file_with_unlicensed_manifests_fails():
    readme = "## License\n\nLicensed under the MPL-2.0.\n"
    assert verdict(tree(readme, True, "UNLICENSED", "UNLICENSED")) == 1


def test_manifests_that_disagree_fail():
    readme = "## License\n\nLicensed under the MPL-2.0.\n"
    assert verdict(tree(readme, True, "MPL-2.0", "Apache-2.0")) == 1


def test_readme_that_does_not_name_the_licence_fails():
    readme = "## License\n\nSee the LICENSE file.\n"
    assert verdict(tree(readme, True, "MPL-2.0", "MPL-2.0")) == 1


def test_a_licence_in_the_manifests_without_a_file_fails():
    assert verdict(tree(ALL_RIGHTS, False, "MPL-2.0", "MPL-2.0")) == 1


if __name__ == "__main__":
    tests = [
        test_no_licence_anywhere_passes,
        test_one_licence_everywhere_passes,
        test_licence_file_with_an_all_rights_reserved_readme_fails,
        test_licence_file_with_unlicensed_manifests_fails,
        test_manifests_that_disagree_fail,
        test_readme_that_does_not_name_the_licence_fails,
        test_a_licence_in_the_manifests_without_a_file_fails,
    ]
    failed = 0
    for test in tests:
        try:
            test()
            print(f"PASS: {test.__name__}")
        except AssertionError as e:
            print(f"FAIL: {test.__name__}: {e}")
            failed += 1
    if failed:
        print(f"\n{failed} test(s) failed")
        sys.exit(1)
    print(f"\nAll {len(tests)} tests passed")
