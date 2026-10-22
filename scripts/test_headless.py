#!/usr/bin/env python3
"""Run the workspace harness with disposable application config/data roots."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def isolated_headless_harness():
    subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "scripts",
                    "-p", "test_*.py"], check=True,
                   cwd=Path(__file__).resolve().parents[1])
    with tempfile.TemporaryDirectory(prefix="clef-headless-") as directory:
        root = Path(directory)
        env = os.environ.copy()
        # The simulator must not initialize a GPU in a headless session.
        env["ICED_TEST_BACKEND"] = "tiny-skia"
        # Keep HOME/CARGO_HOME/RUSTUP_HOME: toolchain caches are not application data.
        for variable in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
            path = root / variable.lower()
            path.mkdir()
            env[variable] = str(path)
        for variable in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS",
                         "DATABASE_URL"):
            env.pop(variable, None)
        subprocess.run(["cargo", "test", "--workspace", "--locked"],
                       env=env, check=True, cwd=Path(__file__).resolve().parents[1])
    print("isolated_headless_harness: PASS")


if __name__ == "__main__":
    isolated_headless_harness()
