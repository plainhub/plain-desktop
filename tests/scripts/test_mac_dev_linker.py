import os
from pathlib import Path
import subprocess
import tempfile
import unittest


LINKER = Path(__file__).resolve().parents[2] / "src-tauri/scripts/mac-dev-linker.sh"


class MacDevLinkerTests(unittest.TestCase):
    def run_linker(self, *, identity=True, linker_status=0, signing_status=0, app=True, signing_warning=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = {
                "cc": 'printf "%s\\n" "$@" > "$LINKER_ARGS"\n'
                'printf "%s" "$LINKER_MESSAGE" >&2\nexit "$LINKER_STATUS"\n',
                "security": 'printf "%s" "$SIGNING_IDENTITY"\n',
                "codesign": 'printf "%s\\n" "$@" > "$SIGNING_ARGS"\n'
                'if [ -n "$SIGNING_MESSAGE" ]; then\n'
                'printf "%s" "$SIGNING_MESSAGE" >&2\nelse\n'
                'printf "%s: replacing existing signature\\n" "${!#}" >&2\nfi\n'
                'exit "$SIGNING_STATUS"\n',
            }
            for name, body in scripts.items():
                executable = root / name
                executable.write_text("#!/bin/bash\n" + body)
                executable.chmod(0o755)
            env = os.environ | {
                "PATH": str(root) + os.pathsep + os.environ["PATH"],
                "LINKER_ARGS": str(root / "linker-args"),
                "SIGNING_ARGS": str(root / "signing-args"),
                "LINKER_STATUS": str(linker_status),
                "SIGNING_STATUS": str(signing_status),
                "SIGNING_IDENTITY": '1) "Apple Development: Test"\n' if identity else "",
                "LINKER_MESSAGE": "linker failed\n" if linker_status else "",
                "SIGNING_MESSAGE": "signing failed\n" if signing_status else "signing warning\n" if signing_warning else "",
            }
            output = root / ("target/debug/deps/plainapp_desktop-test" if app else "other")
            result = subprocess.run(
                ["/bin/bash", str(LINKER), "-o", str(output)],
                env=env,
                capture_output=True,
                text=True,
            )
            args = (root / "linker-args").read_text().splitlines()
            signing_args = root / "signing-args"
            signed = signing_args.read_text().splitlines() if signing_args.exists() else None
            return result, args, signed

    def test_successful_signing_is_silent_and_retains_stable_identity(self):
        result, args, signed = self.run_linker()
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, "", ""))
        self.assertIn("-Wl,-headerpad,0x8000", args)
        self.assertIn("Apple Development: Test", signed)
        self.assertIn("com.ismartcoding.plain.desktop.dev", signed)

    def test_signing_failure_preserves_diagnostic_and_exit_status(self):
        result, _, _ = self.run_linker(signing_status=7)
        self.assertEqual(result.returncode, 7)
        self.assertIn("signing failed", result.stderr)

    def test_linker_failure_is_preserved_without_signing(self):
        result, _, signed = self.run_linker(linker_status=12)
        self.assertEqual((result.returncode, result.stderr), (12, "linker failed\n"))
        self.assertIsNone(signed)

    def test_unexpected_success_diagnostics_are_preserved(self):
        result, _, _ = self.run_linker(signing_warning=True)
        self.assertEqual(result.returncode, 0)
        self.assertIn("signing warning", result.stderr)

    def test_no_identity_keeps_default_signature_without_warning(self):
        result, _, signed = self.run_linker(identity=False)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, "", ""))
        self.assertIsNone(signed)

    def test_other_binaries_use_the_default_linker_without_signing(self):
        result, args, signed = self.run_linker(app=False)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, "", ""))
        self.assertNotIn("-Wl,-headerpad,0x8000", args)
        self.assertIsNone(signed)


if __name__ == "__main__":
    unittest.main()
