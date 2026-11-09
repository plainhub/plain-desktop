import os
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch


SUPPORT = Path(__file__).resolve().parents[1] / 'fixtures/native-media'
sys.dont_write_bytecode = True
sys.path.insert(0, str(SUPPORT))
import generate
import runner


class NativeMediaSupportTests(unittest.TestCase):
    def test_non_linux_refuses_before_any_process_or_root_creation(self):
        with patch.object(runner.sys, 'platform', 'darwin'), patch.object(runner.tempfile, 'mkdtemp') as create, \
                patch.object(runner.subprocess, 'Popen') as launch:
            with self.assertRaisesRegex(RuntimeError, 'NOT RUN'):
                runner.run('/not-used', 'not-used')
            create.assert_not_called()
            launch.assert_not_called()

    def test_non_hosted_linux_refuses_before_any_process_or_root_creation(self):
        with patch.object(runner.sys, 'platform', 'linux'), patch.dict(os.environ, {}, clear=True), \
                patch.object(runner.tempfile, 'mkdtemp') as create, patch.object(runner.subprocess, 'Popen') as launch:
            with self.assertRaisesRegex(RuntimeError, 'NOT RUN'):
                runner.run('/not-used', 'not-used')
            create.assert_not_called()
            launch.assert_not_called()

    def test_xauthority_contains_only_the_owned_cookie_and_display(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'authority'
            cookie = bytes(range(16))
            runner.authority(path, '123', cookie)
            data = path.read_bytes()
            self.assertEqual(struct.unpack_from('>H', data)[0], 65535)
            cursor, fields = 2, []
            while cursor < len(data):
                size = struct.unpack_from('>H', data, cursor)[0]
                cursor += 2
                fields.append(data[cursor:cursor + size])
                cursor += size
            self.assertEqual(fields, [b'', b'123', b'MIT-MAGIC-COOKIE-1', cookie])
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_tampered_seed_is_rejected(self):
        with patch.object(Path, 'read_bytes', return_value=b'changed'):
            with self.assertRaisesRegex(ValueError, 'identity changed'):
                generate.video(Path('/not-written'))

    def test_audio_evidence_requires_rendered_buffers_not_a_placeholder(self):
        self.assertFalse(runner.audio_evidence('created <fake-audio-sink>')['selection_observed'])
        self.assertFalse(runner.audio_evidence('<pulsesink> rendering object 0x123')['selection_observed'])
        observed = runner.audio_evidence('<fake-audio-sink> rendering object 0x123')
        self.assertTrue(observed['selection_observed'])
        self.assertEqual(observed['rendered_fake_sink_buffers'], 1)

    def test_http_resource_urls_are_redacted_from_diagnostic_output(self):
        self.assertEqual(runner.redacted_log('failed http://127.0.0.1/fs?id=opaque&action=preview end'),
                         'failed [HTTP resource URL omitted] end')

    def test_large_fixtures_contain_real_samples(self):
        with tempfile.TemporaryDirectory() as root:
            result = generate.generate(root)
            self.assertEqual(result['video.webm']['sha256'], 'ce0b717401afcbb7e258a2ecd9c01b9f043a5125b2ba62b5e1155a9c18f3510e')
            self.assertEqual(result['video.webm']['sample_bytes'], 54425472)
            self.assertEqual(result['video.webm']['duration'], 64)
            self.assertGreater(result['audio.wav']['sample_bytes'], 32 * 1024 * 1024)
            self.assertEqual(result['small-video.webm']['duration'], 8)
            self.assertLess(result['small-video.webm']['size'], 32 * 1024 * 1024)
            self.assertEqual(result['small-video.webm']['expected_route'], 'production-native-scheme')
            self.assertEqual(result['video.webm']['expected_route'], 'production-core-http-fs')
            self.assertEqual((Path(root) / 'control.wav').stat().st_size, 4 * 48000 * 4 + 44)


if __name__ == '__main__':
    unittest.main()
