#!/usr/bin/env python3
"""Audit direct dependencies offline; optionally verify the upgrade snapshot online."""
import argparse
import json
from pathlib import Path
import subprocess
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT = json.loads((ROOT / "scripts/dependency_snapshot.json").read_text())


def dependency_targets_match_registry_snapshot():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT,
        text=True,
    ))
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    targets = SNAPSHOT["targets"]
    seen = set()
    errors = []
    for member in metadata["workspace_members"]:
        package = packages[member]
        node = nodes[member]
        for dependency in package["dependencies"]:
            if dependency["source"] is None:
                continue
            name = dependency["name"]
            seen.add(name)
            if name not in targets:
                errors.append(f"{package['name']}: unexpected direct dependency {name}")
                continue
            expected = targets[name]
            if dependency["req"] != f"^{expected}":
                errors.append(f"{package['name']}: {name} requirement {dependency['req']} != ^{expected}")
            alias = (dependency["rename"] or name).replace("-", "_")
            resolutions = [packages[edge["pkg"]] for edge in node["deps"]
                           if edge["name"] == alias]
            if not resolutions or any(p["version"] != expected for p in resolutions):
                errors.append(f"{package['name']}: {name} does not resolve to {expected}")
    if missing := targets.keys() - seen:
        errors.append(f"snapshot dependencies missing from manifests: {sorted(missing)}")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"dependency_targets_match_registry_snapshot: PASS ({SNAPSHOT['verified_on']})")
    print("locked_resolution: PASS (cargo metadata --locked)")


def verify_registry():
    """Upgrade-time check only; routine CI does not require a live registry query."""
    for name, expected in SNAPSHOT["targets"].items():
        url = f"https://crates.io/api/v1/crates/{name}"
        request = urllib.request.Request(url, headers={"User-Agent": "clef-dependency-audit"})
        with urllib.request.urlopen(request, timeout=60) as response:
            crate = json.load(response)
        stable = [version for version in crate["versions"]
                  if not version["yanked"] and "-" not in version["num"]]
        latest = max(stable, key=lambda version: tuple(map(int, version["num"].split("+", 1)[0].split("."))))
        if latest["num"] != expected:
            raise SystemExit(f"{name}: registry latest stable {latest['num']} != snapshot {expected}")
        print(f"{name} {expected}: stable, non-yanked; {url}/{expected}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-registry", action="store_true",
                        help="also compare with the live registry (upgrade time only)")
    args = parser.parse_args()
    if args.verify_registry:
        verify_registry()
    dependency_targets_match_registry_snapshot()
