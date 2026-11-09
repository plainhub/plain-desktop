import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib
from unittest.mock import patch


SUPPORT = Path(__file__).resolve().parents[1] / 'fixtures/native-media'
sys.dont_write_bytecode = True
sys.path.insert(0, str(SUPPORT))
import generate
import runner


class NativeMediaSupportTests(unittest.TestCase):
    def test_page_transport_and_async_media_contracts(self):
        script = r'''
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(process.argv[1], 'utf8').replace('FIXTURE_URLS', '{"unitOnly":true}');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
function setup(invoke) {
  const context = { window: { __TAURI_INTERNALS__: { invoke } }, TextEncoder, DOMException,
    location: { href: 'tauri://localhost/native-media-test.html' }, performance,
    HTMLMediaElement: { NETWORK_EMPTY: 0, HAVE_NOTHING: 0 }, setTimeout, clearTimeout, setInterval, clearInterval,
    document: new Proxy({}, { set() { throw Error('Title transport must not be used'); } }) };
  vm.runInNewContext(source, context);
  return { api: context.window.__plainNativeTestSupport, state: context.window.__plainTransportState };
}
function emptyMedia() {
  return { tagName: 'AUDIO', networkState: 0, readyState: 0, paused: true, seeking: false, ended: false,
    currentTime: 0, currentSrc: '', srcObject: null, hasAttribute: () => false, querySelectorAll: () => [],
    buffered: { length: 0 }, seekable: { length: 0 }, addEventListener() {}, removeEventListener() {} };
}
function video(deliverFrames = true, distinct = true) {
  const callbacks = new Map();
  let next = 0, quality = 100;
  const media = { ...emptyMedia(), tagName: 'VIDEO', currentTime: 58, paused: true,
    getVideoPlaybackQuality: () => ({ totalVideoFrames: quality }),
    requestVideoFrameCallback(fn) { callbacks.set(++next, fn); return next; },
    cancelVideoFrameCallback(id) { callbacks.delete(id); },
    pause() { this.paused = true; },
    play() {
      this.paused = false;
      const frame = (time, count) => {
        quality = count;
        if (!deliverFrames) return;
        const entry = callbacks.entries().next().value;
        if (entry) { callbacks.delete(entry[0]); entry[1](performance.now(), { mediaTime: time, presentedFrames: count }); }
      };
      setTimeout(() => { this.currentTime = 58.4; frame(58.05, 1); }, 5);
      setTimeout(() => frame(distinct ? 58.35 : 58.05, 2), 25);
      return Promise.resolve();
    }
  };
  return media;
}
(async () => {
  const calls = [];
  let acknowledge;
  const transport = setup(async (name, { packet }) => {
    assert.equal(name, 'native_media_report');
    calls.push(JSON.parse(JSON.stringify(packet)));
    if (calls.length === 1) return new Promise(resolve => { acknowledge = resolve; });
    return packet.sequence;
  });
  const first = transport.api.transmit('result', { detail: 'x'.repeat(3000) });
  const second = transport.api.transmit('result', { detail: 'y'.repeat(3000) });
  await sleep(0);
  assert.equal(calls.length, 1, 'Only one acknowledgement may be in flight');
  assert.equal(transport.state().pending, 2);
  assert.equal(transport.state().acknowledged, 0);
  acknowledge(1);
  assert.equal(await first, true);
  assert.equal(await second, true);
  assert.deepEqual(calls.map(call => call.sequence), [1, 2]);
  assert.equal(transport.state().pending, 0);
  assert.equal(transport.state().failure, null);
  assert.equal(transport.state().acknowledged, transport.state().sequence);
  assert.equal(calls[1].payload.detail.length, 3000, 'Reports above title length must remain intact');
  const oversize = setup(async () => { throw Error('Oversized packet was transmitted'); });
  assert.equal(await oversize.api.transmit('result', { detail: 'x'.repeat(16384) }), false);
  assert.match(oversize.state().failure, /bound/);
  const wrongAck = setup(async () => 0);
  assert.equal(await wrongAck.api.transmit('event', {}), false);
  assert.match(wrongAck.state().failure, /sequence/);
  const overflow = setup(async (_, { packet }) => packet.sequence);
  await Promise.all(Array.from({ length: 257 }, () => overflow.api.transmit('diagnostic', {})));
  assert.match(overflow.state().failure, /bound/);
  const ordered = setup(async (_, { packet }) => packet.sequence);
  for (let i = 0; i < 512; i++) assert.equal(await ordered.api.transmit('diagnostic', {}), true);
  assert.equal(await ordered.api.transmit('diagnostic', {}), false);
  const api = setup(async (_, { packet }) => packet.sequence).api;
  const rendered = await api.advance(video(), 200);
  assert.equal(rendered.quality_before, 100);
  assert.equal(rendered.quality_after, 2, 'A seek-reset counter is not a no-frame failure');
  assert.equal(rendered.video_callbacks, 2);
  assert.ok(rendered.last_frame.media_time > rendered.first_frame.media_time);
  await assert.rejects(api.advance(video(false), 40), /actual video frames/);
  await assert.rejects(api.advance(video(true, false), 40), /actual video frames/);
  const unsupported = video(); delete unsupported.requestVideoFrameCallback;
  await assert.rejects(api.advance(unsupported, 40), /callbacks unavailable/);
  const pendingObservation = {};
  let rejectLate;
  const pending = { ...emptyMedia(), play() { this.currentTime += .5; this.paused = false; return new Promise((_, reject) => { rejectLate = reject; }); } };
  await assert.rejects(api.advance(pending, 300, pendingObservation), /Timeout: Media play/);
  assert.equal(pendingObservation.play_promise, 'pending', 'Time advancement does not replace the play promise');
  assert.equal(pendingObservation.after_time, .5, 'Timeout preserves actual progress');
  assert.ok(pendingObservation.trace.samples.length > 0, 'Progress is sampled while the promise remains pending');
  const frozenObservation = JSON.stringify(pendingObservation);
  rejectLate(Error('cleanup rejected old play promise'));
  await sleep(0);
  assert.equal(JSON.stringify(pendingObservation), frozenObservation, 'Late cleanup cannot rewrite timeout evidence');
  const media = { ...emptyMedia(), networkState: 3, currentSrc: 'old-resource' };
  setTimeout(() => { media.networkState = 0; }, 20);
  const cancelled = await api.released(media, 100, 'old-resource', {}, 20);
  assert.equal(cancelled.final.network_state, 0);
  assert.equal(cancelled.final.current_src, 'original', 'A stale URL alone is not a loaded resource');
  const failedCancel = {};
  await assert.rejects(api.released({ ...media, networkState: 3 }, 30, '', failedCancel, 10), /retained/);
  assert.equal(failedCancel.final.network_state, 3, 'Timeout preserves the final state');
  for (const change of [{ paused: false }, { seeking: true }, { srcObject: {} },
    { hasAttribute: () => true }, { querySelectorAll: () => [{}] },
    { buffered: { length: 1, start: () => 0, end: () => 1 } },
    { seekable: { length: 1, start: () => 0, end: () => 1 } }]) {
    await assert.rejects(api.released({ ...media, ...change }, 15, '', {}, 10), /retained/);
  }
  const moving = { ...media }; setTimeout(() => { moving.currentTime = 1; }, 5);
  await assert.rejects(api.released(moving, 30, '', {}, 15), /continued/);
  const liveFrame = { ...media, tagName: 'VIDEO',
    requestVideoFrameCallback(fn) { setTimeout(fn, 5); return 1; }, cancelVideoFrameCallback() {} };
  await assert.rejects(api.released(liveFrame, 30, '', {}, 15), /continued/);
  const ranges = { count: 4, ranges: Array.from({ length: 4 }, (_, i) =>
    [1.2345678901234567 + i, 190.12345678901235 + i]) };
  const state = { ...rendered.initial, current_time: 210, buffered: ranges, seekable: ranges };
  const worst = { ...rendered, initial: state, final: state, at_promise_settlement: state,
    promise_settled_ms: 12000, elapsed_ms: 24000, trace: {
      events: Array.from({ length: 40 }, () => ({ event: 'canplaythrough', ms: 24000,
        time: 193.12345678901235, network: 3, ready: 4, paused: false, seeking: false, ended: false })),
      samples: Array.from({ length: 16 }, () => ({ ms: 24000, time: 193.12345678901235,
        network: 3, ready: 4, paused: false, seeking: false, ended: false })),
      event_count: 99999, sample_count: 96, records_truncated: true } };
  const packetSize = payload => Buffer.byteLength(JSON.stringify({ sequence: 512, kind: 'diagnostic', payload }));
  const observationBytes = packetSize({ fixture: 'small-video.webm', phase: 'seek:58.88', ...worst });
  const { trace, ...summary } = worst;
  const resultBytes = packetSize({ fixture: 'small-video.webm', kind: 'video', route: 'production-core-http-fs',
    verification_method: 'real-webview-media-events', status: 'failed', stages: ['metadata','play-pause','seek-play-pause:58.88','seek-play-pause:6.4','cancel','reload-play'],
    playback_evidence: Array.from({ length: 4 }, () => ({ phase: 'seek:58.88', ...summary })),
    cancel_after_emptied: state, cancel_observation: { initial: state, final: state, stability_ms: 300, new_video_frames: 0 },
    failure_state: state, error: 'Error: Cancelled media continued to advance' });
  assert.ok(observationBytes < 16384, 'Maximum observation must fit one packet');
  assert.ok(resultBytes < 16384, 'Four detailed playback summaries must fit one result packet');
  console.log('Bounded packet bytes:', observationBytes, resultBytes);
  console.log('Transport ordering/bounds and asynchronous media helper contracts passed; not native GUI acceptance');
})().catch(error => { console.error(error); process.exitCode = 1; });
'''
        subprocess.run(['node', '-e', script, str(SUPPORT / 'page.js')], check=True, timeout=10)

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

    def test_png_contains_independent_pixels_and_valid_chunks(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'image.png'
            info = generate.image(path)
            data = path.read_bytes()
            self.assertEqual(data[:8], b'\x89PNG\r\n\x1a\n')
            chunks, cursor = {}, 8
            while cursor < len(data):
                size = struct.unpack_from('>I', data, cursor)[0]
                kind = data[cursor + 4:cursor + 8]
                payload = data[cursor + 8:cursor + 8 + size]
                self.assertEqual(struct.unpack_from('>I', data, cursor + 8 + size)[0],
                                 zlib.crc32(kind + payload))
                chunks[kind] = payload
                cursor += size + 12
            self.assertEqual(list(chunks), [b'IHDR', b'IDAT', b'IEND'])
            self.assertEqual(struct.unpack('>IIBBBBB', chunks[b'IHDR']), (32, 24, 8, 2, 0, 0, 0))
            pixels = zlib.decompress(chunks[b'IDAT'])
            self.assertEqual(len(pixels), (32 * 3 + 1) * 24)
            self.assertEqual(pixels[:7], b'\x00\x00\x00\x00\x07\x00\x04')
            self.assertEqual(pixels[-3:], bytes((217, 207, 216)))
            self.assertEqual(info['sample_bytes'], 32 * 24 * 3)

    def test_audio_evidence_requires_rendered_buffers_not_a_placeholder(self):
        self.assertFalse(runner.audio_evidence('created <fake-audio-sink>')['selection_observed'])
        self.assertFalse(runner.audio_evidence('<pulsesink> rendering object 0x123')['selection_observed'])
        observed = runner.audio_evidence('<fake-audio-sink> rendering object 0x123')
        self.assertTrue(observed['selection_observed'])
        self.assertEqual(observed['rendered_fake_sink_buffers'], 1)

    def test_pipeline_evidence_survives_json_tail_and_redacts_urls(self):
        line = '0:00:01.123 123 thread DEBUG webkitmediaplayer player:1 <regular-plain-native-audio-wav-2> [Seek] http://127.0.0.1/fs?id=secret'
        log = '\n'.join([line] * 100) + '\n' + 'Native result: ' + 'x' * 20000
        result = runner.pipeline_evidence(log)
        group = result['pipelines']['regular-plain-native-audio-wav-2']
        self.assertEqual(group['matched_lines'], 100)
        self.assertEqual(len(group['retained_lines']), 32)
        self.assertEqual(len(group['seek_lines']), 16)
        other = line.replace('audio-wav-2', 'video-webm-4')
        combined = runner.pipeline_evidence(log + '\n' + '\n'.join([other] * 100))
        self.assertIn('regular-plain-native-audio-wav-2', combined['pipelines'])
        self.assertIn('regular-plain-native-video-webm-4', combined['pipelines'])
        self.assertNotIn('secret', str(result))
        self.assertNotIn('Native result:', str(result))

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
            self.assertEqual(result['small-video.webm']['expected_route'], 'production-core-http-fs')
            self.assertEqual(result['video.webm']['expected_route'], 'production-core-http-fs')
            self.assertEqual(result['image.png']['expected_route'], 'production-native-scheme')
            self.assertEqual((result['image.png']['width'], result['image.png']['height']), (32, 24))
            self.assertEqual((Path(root) / 'control.wav').stat().st_size, 4 * 48000 * 4 + 44)


if __name__ == '__main__':
    unittest.main()
