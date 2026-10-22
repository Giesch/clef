"""Unit tests for the dependency audit; no Cargo invocation or network required."""
import contextlib
import io
import json
import unittest
from unittest.mock import patch

import check_dependencies as audit

URL = "https://github.com/pop-os/libcosmic.git"
REV = "430f3d48a97d8779f9c1985473912e594ccca1ae"


class DependencyAuditTests(unittest.TestCase):
    def metadata(self, *, requirement="^0.25.10", resolved="0.25.10", name="image"):
        return {
            "workspace_members": ["clef_ui"],
            "packages": [
                {"id": "clef_ui", "name": "clef_ui", "dependencies": [
                    {"name": name, "source": "registry+crates.io", "rename": "image_rs",
                     "req": requirement},
                ]},
                {"id": "image", "name": name, "version": resolved},
                # An older transitive copy is not a direct dependency failure.
                {"id": "image-old", "name": name, "version": "0.24.0"},
            ],
            "resolve": {"nodes": [
                {"id": "clef_ui", "deps": [{"name": "image_rs", "pkg": "image"}]},
            ]},
        }

    def run_audit(self, metadata):
        with patch.object(audit, "SNAPSHOT", {
            "verified_on": "2026-10-16", "targets": {"image": "0.25.10"},
        }), patch.object(audit.subprocess, "check_output", return_value=json.dumps(metadata)), \
                contextlib.redirect_stdout(io.StringIO()):
            audit.dependency_targets_match_registry_snapshot()

    def test_alias_and_older_transitive_are_allowed(self):
        self.run_audit(self.metadata())

    def test_stale_requirement_is_rejected(self):
        with self.assertRaisesRegex(SystemExit, "requirement"):
            self.run_audit(self.metadata(requirement="^0.24"))

    def test_stale_resolution_is_rejected(self):
        with self.assertRaisesRegex(SystemExit, "does not resolve"):
            self.run_audit(self.metadata(resolved="0.24.0"))

    def test_obsolete_direct_dependency_is_rejected(self):
        with self.assertRaisesRegex(SystemExit, "unexpected direct dependency iced_native"):
            self.run_audit(self.metadata(name="iced_native"))

    def git_metadata(self, *, source=None, resolved_rev=REV):
        source = source or f"git+{URL}?rev={REV}"
        return {
            "workspace_members": ["clef_ui"],
            "packages": [
                {"id": "clef_ui", "name": "clef_ui", "dependencies": [
                    {"name": "libcosmic", "source": source, "rename": None, "req": "*"},
                ]},
                {"id": "libcosmic", "name": "libcosmic",
                 "source": f"git+{URL}?rev={REV}#{resolved_rev}"},
            ],
            "resolve": {"nodes": [
                {"id": "clef_ui", "deps": [{"name": "cosmic", "pkg": "libcosmic"}]},
            ]},
        }

    def run_git_audit(self, metadata, git=None):
        git = {"libcosmic": {"url": URL, "rev": REV}} if git is None else git
        with patch.object(audit, "SNAPSHOT", {
            "verified_on": "2026-10-16", "targets": {}, "git": git,
        }), patch.object(audit.subprocess, "check_output", return_value=json.dumps(metadata)), \
                contextlib.redirect_stdout(io.StringIO()):
            audit.dependency_targets_match_registry_snapshot()

    def test_pinned_git_dependency_with_library_name_is_allowed(self):
        self.run_git_audit(self.git_metadata())

    def test_unpinned_git_source_is_rejected(self):
        with self.assertRaisesRegex(SystemExit, "source"):
            self.run_git_audit(self.git_metadata(source=f"git+{URL}?branch=master"))

    def test_git_resolution_at_other_commit_is_rejected(self):
        with self.assertRaisesRegex(SystemExit, "does not resolve"):
            self.run_git_audit(self.git_metadata(resolved_rev="0" * 40))

    def test_unexpected_git_dependency_is_rejected(self):
        with self.assertRaisesRegex(SystemExit, "unexpected git dependency libcosmic"):
            self.run_git_audit(self.git_metadata(), git={})


if __name__ == "__main__":
    unittest.main()
