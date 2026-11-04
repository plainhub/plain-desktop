import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib


root = Path(__file__).resolve().parents[1]
source = Path(os.environ.get("GLIB_BACKPORT_SOURCE", root / "vendor/glib"))
locked = tomllib.loads((root / "Cargo.lock").read_text())
allowed = {(p["name"], p["version"]) for p in locked["package"]}
with tempfile.TemporaryDirectory(prefix="plain-glib-regression-") as directory:
    work = Path(directory)
    (work / "Cargo.toml").write_text(
        '[package]\nname = "plain-glib-regression"\nversion = "0.0.0"\nedition = "2021"\n'
        f'[dependencies]\nglib = {{ path = {json.dumps(str(source))} }}\n'
        '[[test]]\nname = "variant"\n'
        f'path = {json.dumps(str(root / "tests/security/glib_variant.rs"))}\n'
    )
    shutil.copyfile(root / "Cargo.lock", work / "Cargo.lock")
    subprocess.run(["cargo", "metadata", "--offline", "--format-version", "1"], cwd=work, check=True, stdout=subprocess.DEVNULL)
    actual = tomllib.loads((work / "Cargo.lock").read_text())
    added = {(p["name"], p["version"]) for p in actual["package"]} - allowed - {("plain-glib-regression", "0.0.0")}
    if added:
        raise SystemExit(f"Unexpected dependencies: {sorted(added)}")
    subprocess.run(["cargo", "test", "--offline", "--locked", "--release", "--test", "variant"], cwd=work, check=True)
