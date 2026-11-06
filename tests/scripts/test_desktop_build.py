import json
from pathlib import Path
import shlex
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]


class DesktopBuildTests(unittest.TestCase):
    def test_dev_business_packages_skip_optimizer(self):
        profile = tomllib.loads((ROOT / "Cargo.toml").read_text())["profile"]["dev"]
        self.assertEqual(profile["opt-level"], 0)
        self.assertTrue(profile["incremental"])
        for name in ["plain-rs", "plain-server"]:
            self.assertEqual(profile["package"][name]["opt-level"], 0, name)
        self.assertEqual(profile["package"]["plain-libs"]["opt-level"], 1)
        self.assertEqual(profile["package"]["*"]["opt-level"], 3)

    def test_shell_only_builds_the_rust_library_consumed_by_the_executable(self):
        manifest = tomllib.loads((ROOT / "src-tauri/Cargo.toml").read_text())
        self.assertEqual(manifest["lib"].get("crate-type", ["rlib"]), ["rlib"])

    def test_preview_packages_without_release_optimization(self):
        scripts = json.loads((ROOT / "package.json").read_text())["scripts"]
        self.assertEqual(shlex.split(scripts["build:tauri:preview"]), ["tauri", "build", "--debug"])
        self.assertEqual(shlex.split(scripts["build:tauri"]), ["tauri", "build"])


if __name__ == "__main__":
    unittest.main()
