"""Verify test environment isolation without launching the Rust harness."""
import contextlib
import io
import os
from pathlib import Path
import unittest
from unittest.mock import patch

import test_headless


class HeadlessRunnerTests(unittest.TestCase):
    def test_isolated_headless_harness(self):
        calls = []

        def record(command, **kwargs):
            calls.append((command, kwargs))
            if "env" not in kwargs:
                return
            environment = kwargs["env"]
            self.assertEqual(environment["ICED_TEST_BACKEND"], "tiny-skia")
            for variable in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS",
                             "DATABASE_URL"):
                self.assertNotIn(variable, environment)
            paths = [Path(environment[variable]) for variable in (
                "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME")]
            self.assertTrue(all(path.is_dir() for path in paths))
            self.assertEqual(len({path.parent for path in paths}), 1)

        with patch.dict(os.environ, {"DATABASE_URL": "private.sqlite", "DISPLAY": ":1",
                                     "WAYLAND_DISPLAY": "wayland-0",
                                     "DBUS_SESSION_BUS_ADDRESS": "private-bus"}), \
                patch.object(test_headless.subprocess, "run", side_effect=record), \
                contextlib.redirect_stdout(io.StringIO()):
            test_headless.isolated_headless_harness()
        self.assertEqual(calls[-1][0], ["cargo", "test", "--workspace", "--locked"])
        self.assertTrue(calls[-1][1]["check"])
        self.assertFalse(Path(calls[-1][1]["env"]["XDG_DATA_HOME"]).exists())


if __name__ == "__main__":
    unittest.main()
