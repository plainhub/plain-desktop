import contextlib
import errno
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib
from unittest.mock import MagicMock, patch


SUPPORT = Path(__file__).resolve().parents[1] / 'fixtures/native-media'
sys.dont_write_bytecode = True
sys.path.insert(0, str(SUPPORT))
import generate
import runner


class NativeMediaSupportTests(unittest.TestCase):
    @staticmethod
    def http_line(message, source='webkitwebsrc0', level='DEBUG'):
        return ('0:00:01.123456789 123 0x456 ' + level +
                ' webkitwebsrc WebKitWebSourceGStreamer.cpp:1074:responseReceived:<' + source + '> ' + message)

    def test_http_evidence_parses_gstreamer_scalars_without_publishing_other_headers(self):
        lines = [self.http_line('Posting task to request R1 http://127.0.0.1/fs?id=SECRET requestedPosition=0 stopPosition=18446744073709551615'),
                 self.http_line(r'Range request: bytes=0-'),
                 self.http_line(r'R1: Request headers going downstream: request-headers, Range=(string)"bytes\=0-", Authorization=(string)SECRET;'),
                 self.http_line('R1: Received response: 206'),
                 self.http_line(r'R1: Response headers going downstream: response-headers, Content-Length=(guint64)40320044, Content-Range=(string)"bytes\ 0-40320043/40320044", Accept-Ranges=(string)bytes, Content-Type=(string)audio/wav, Set-Cookie=(string)"SECRET\,\ value", Location=(string)"http://SECRET";'),
                 self.http_line('R1: Have 8192 bytes of data', level='LOG'),
                 self.http_line('R1: Have 4096 bytes of data', level='LOG'),
                 self.http_line('R1: Load finished. Read position: 40320044', level='LOG')]
        result = runner.http_evidence(lines)
        request = result['requests'][0]
        self.assertEqual(request['request_headers'], {'range': 'bytes=0-'})
        self.assertEqual(request['response_headers'], {'content-length': 40320044,
            'content-range': 'bytes 0-40320043/40320044', 'accept-ranges': 'bytes', 'content-type': 'audio/wav'})
        self.assertEqual(request['received_bytes'], 12288)
        self.assertEqual(request['read_position'], 40320044)
        self.assertTrue(request['finished'])
        self.assertEqual(request['data_records'], 2)
        self.assertNotIn('SECRET', json.dumps(result))
        self.assertNotIn('http://', json.dumps(result))
        self.assertNotIn('authorization', json.dumps(result))

    def test_http_evidence_rejects_ambiguous_structures_and_header_injection(self):
        values = [r'response-headers, Content-Length=(guint64)12, Content-Length=(guint64)13;',
                  r'response-headers, Content-Length-SECRET=(guint64)12, X=(string)"unterminated;',
                  r'response-headers, "Content-Length"=(guint64)12;',
                  r'response-headers, Content-Length=(guint64)12, X=(string)"SECRET\nwebkitmediaplayer [Buffering]";',
                  'response-headers, Content-Length=(guint64)12, X=(string)"SECRET\nwebkitmediaplayer [Buffering]";',
                  r'response-headers, Content-Length=(string)"12SECRET", Content-Type=(string)SECRET/type;',
                  r'response-headers, Content-Length=(guint64)12, X=(string)"SECRET\"\,\ Content-Length\=999";',
                  'response-headers, X=(string)' + 'SECRET' * 4000 + ';']
        for value in values:
            with self.subTest(value=value[:80]):
                log = self.http_line('R1: Response headers going downstream: ' + value)
                result = runner.http_evidence(log.splitlines())
                self.assertNotIn('SECRET', json.dumps(result))
                self.assertNotIn('999', json.dumps(result))
                self.assertFalse(runner.pipeline_evidence(runner.legacy_diagnostic_log(log))['pipelines'])
                self.assertFalse(runner.audio_evidence(runner.legacy_diagnostic_log(log))['selection_observed'])
        unknown = runner.http_evidence([self.http_line('R1: Response headers going downstream: unknown SECRET')])
        self.assertEqual(unknown['requests'][0]['response_headers'], 'unobserved')
        self.assertFalse(unknown['observed'])
        self.assertIsNone(unknown['requests'][0]['received_bytes'])
        self.assertEqual(unknown['unobserved_records'], 1)

    def test_http_evidence_preserves_request_identity_seek_cancel_and_bounds(self):
        lines = [self.http_line('Posting task to request R1 http://SECRET requestedPosition=10 stopPosition=20'),
                 self.http_line('Seek segment: (10-20) Position previous to seek: 5'),
                 self.http_line('R1: Request canceled: SECRET', level='LOG'),
                 self.http_line('R1: Have 4 bytes of data', level='LOG', source='another-secret-source'),
                 self.http_line('R1: Have 3 bytes of data', level='LOG')]
        result = runner.http_evidence(lines)
        self.assertEqual([row['received_bytes'] for row in result['requests']], [3, 4])
        self.assertTrue(result['requests'][0]['canceled'])
        self.assertEqual(result['events'][0]['start'], 10)
        self.assertNotIn('SECRET', json.dumps(result))
        self.assertNotIn('secret-source', json.dumps(result))
        bounded = runner.http_evidence([self.http_line(f'R{i}: Received response: 200') for i in range(100)])
        self.assertEqual(len(bounded['requests']), 64)
        self.assertEqual(bounded['omitted_records'], 36)
        sources = runner.http_evidence([self.http_line('R1: Received response: 200', source=f'source{i}') for i in range(25)])
        self.assertEqual(len(sources['requests']), 16)
        self.assertEqual(sources['omitted_records'], 9)
        events = runner.http_evidence([self.http_line('R1: stopping download')] * 200)
        self.assertEqual(len(events['events']), 128)
        self.assertEqual(events['omitted_records'], 72)

    def test_http_headers_cannot_enter_legacy_diagnostics_or_audio_evidence(self):
        injected = self.http_line('R1: Response headers going downstream: response-headers, X=(string)"SECRET webkitmediaplayer basesink [Buffering] <fake-audio-sink> rendering object";')
        injected += '\nSECRET [Seek] fake-audio-sink rendering object\n' + 'SECRET' * 4000
        normal = '0:00:01.123 123 0x456 DEBUG webkitmediaplayer file:1:func:<regular-plain-native-audio-wav-2> [Buffering] mode: GST_BUFFERING_DOWNLOAD, status: 100.000000%'
        normal += '\n0:00:01.124 123 0x456 DEBUG basesink file:1:func:<fake-audio-sink> rendering object 0x123'
        legacy = runner.legacy_diagnostic_log(injected + '\n' + normal)
        self.assertNotIn('SECRET', legacy)
        self.assertTrue(runner.audio_evidence(legacy)['selection_observed'])
        self.assertIn('regular-plain-native-audio-wav-2', runner.pipeline_evidence(legacy)['pipelines'])

    def test_failed_run_does_not_print_or_retain_raw_header_logs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'owned'
            root.mkdir(mode=0o700)
            def generated(path):
                for name in ['native.log', 'dbus.log', 'xvfb.log']:
                    (path / name).write_text(self.http_line('R1: Response headers going downstream: response-headers, Authorization=(string)SECRET;'))
                raise RuntimeError('Synthetic setup failure')
            output = io.StringIO()
            read_text = Path.read_text
            def host_text(path, *args, **kwargs):
                if str(path) in ('/var/lib/dpkg/status', '/etc/os-release'):
                    return ''
                return read_text(path, *args, **kwargs)
            with patch.object(Path, 'read_text', host_text), \
                 patch.object(runner, 'audio_configuration', return_value=('', [])), \
                 patch.object(runner, 'admit', return_value={}), \
                 patch.object(runner.tempfile, 'mkdtemp', return_value=str(root)), \
                 patch.object(runner, 'owned_mounts', return_value=[]), \
                 patch.object(runner, 'cleanup_portal_mount'), \
                 patch.object(runner, 'generate', side_effect=generated) as generate_mock, contextlib.redirect_stdout(output):
                self.assertEqual(runner.run('/unused', 'unused'), 1)
                generate_mock.assert_called_once_with(root.resolve())
            self.assertNotIn('SECRET', output.getvalue())
            self.assertNotIn('SECRET', (root / 'report.json').read_text())
            for name in ['native.log', 'dbus.log', 'xvfb.log']:
                self.assertFalse((root / name).exists())

    def test_buffering_deferral_accepts_only_exact_upstream_trace(self):
        line = ('0:00:01.123456789 123 0x456 TRACE webkitmediaplayer player.cpp:3036:updateStates:'
                '<regular-plain-native-audio-wav-2> [Buffering] Delaying m_isBuffering false --> true '
                'to force the proper change from not buffering to buffering when the async state change completes.')
        result = runner.buffering_evidence([line] * 40)
        self.assertEqual(result['matched_records'], 40)
        self.assertEqual(len(result['events']), 32)
        self.assertEqual(result['events'][0]['previous'], False)
        self.assertEqual(result['events'][0]['current'], True)
        self.assertFalse(runner.buffering_evidence([line + ' SECRET', line.replace('TRACE', 'DEBUG'),
            self.http_line('R1: Response headers going downstream: ' + line)])['observed'])
        self.assertNotIn('SECRET', json.dumps(runner.buffering_evidence([line + ' SECRET'])))
        self.assertEqual(runner.legacy_diagnostic_log(line), '')

    def test_downstream_http_header_logging_and_forged_continuations_are_not_diagnostics(self):
        downstream = ('0:00:01.123 123 0x456 DEBUG webkitmediaplayer MediaPlayerPrivateGStreamer.cpp:2321:handleMessage:'
            '<regular-plain-native-audio-wav-2> Processing HTTP headers: http-headers, '
            'request-headers=(structure)"request-headers, Authorization=(string)SECRET_segment_fake-audio-sink rendering object;";')
        multiline = ('0:00:01.123 100 0x456 DEBUG webkitwebsrc file:1:f:<webkitwebsrc0> R1: Request canceled: SECRET\n'
            '0:00:01.124 100 0x456 DEBUG basesink file:1:f:<fake-audio-sink> [Seek] rendering object SECRET')
        for log in [downstream, multiline]:
            safe = runner.legacy_diagnostic_log(log)
            self.assertEqual(safe, '')
            self.assertNotIn('SECRET', json.dumps(runner.pipeline_evidence(safe)))
            self.assertFalse(runner.audio_evidence(safe)['selection_observed'])
        valid = '0:00:01.124 100 0x456 DEBUG basesink file:1:f:<fake-audio-sink> rendering object 0x123'
        self.assertTrue(runner.audio_evidence(runner.legacy_diagnostic_log(valid))['selection_observed'])
        self.assertFalse(runner.audio_evidence(runner.legacy_diagnostic_log(valid + ' SECRET'))['selection_observed'])

    def test_http_source_identity_includes_process_and_unknown_records_are_unobserved(self):
        lines = [self.http_line('R1: Have 4 bytes of data', level='LOG'),
                 self.http_line('R1: Have 8 bytes of data', level='LOG').replace(' 123 ', ' 456 ')]
        result = runner.http_evidence(lines)
        self.assertEqual([row['received_bytes'] for row in result['requests']], [4, 8])
        self.assertEqual([row['source'] for row in result['requests']], [1, 2])
        unknown = runner.http_evidence([self.http_line('R1: unfamiliar SECRET')])
        self.assertFalse(unknown['observed'])
        self.assertEqual(unknown['unobserved_records'], 1)
        self.assertEqual(unknown['requests'], [])
        self.assertIsNone(runner.http_header_fields('response-headers, Content-Length=(guint64)12, ;', 'response'))

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
  const canceled = await api.released(media, 100, 'old-resource', {}, 20);
  assert.equal(canceled.final.network_state, 0);
  assert.equal(canceled.final.current_src, 'original', 'A stale URL alone is not a loaded resource');
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
    failure_state: state, error: 'Error: Canceled media continued to advance' });
  assert.ok(observationBytes < 16384, 'Maximum observation must fit one packet');
  assert.ok(resultBytes < 16384, 'Four detailed playback summaries must fit one result packet');
  console.log('Bounded packet bytes:', observationBytes, resultBytes);
  console.log('Transport ordering/bounds and asynchronous media helper contracts passed; not native GUI acceptance');
})().catch(error => { console.error(error); process.exitCode = 1; });
'''
        subprocess.run(['node', '-e', script, str(SUPPORT / 'page.js')], check=True, timeout=10)

    def test_disconnected_owned_portal_is_unmounted_before_directory_removal(self):
        self._run_owned_portal_cleanup()

    def test_runner_rejects_directory_changes_during_unmount(self):
        for name in ['', 'home', 'runtime', 'config', 'cache', 'data']:
            for change in ['replace', 'permissions']:
                with self.subTest(directory=name, change=change):
                    self._run_owned_portal_cleanup(('unmount', name, change))

    def test_runner_rechecks_remaining_directories_before_removal(self):
        for name in ['', 'runtime', 'config', 'cache', 'data']:
            for change in ['replace', 'permissions']:
                with self.subTest(directory=name, change=change):
                    self._run_owned_portal_cleanup(('after-home-removal', name, change))

    def _run_owned_portal_cleanup(self, mutation=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve() / 'owned'
            root.mkdir(mode=0o700)
            mounted = False
            original_read = Path.read_text
            original_remove = runner.shutil.rmtree
            children = [MagicMock(pid=101), MagicMock(pid=102), MagicMock(pid=103, returncode=0)]
            children[0].poll.return_value = children[1].poll.return_value = None
            children[-1].poll.side_effect = [None, 0]

            def change_directory(stage):
                if mutation is None or mutation[0] != stage:
                    return
                path = root / mutation[1]
                if mutation[2] == 'replace':
                    path.rename(path.with_name('retired-' + path.name))
                    path.mkdir(mode=0o700)
                else:
                    path.chmod(0o755)
                (path / 'retained.txt').write_text('Replacement must not be removed')

            def read(path, *args, **kwargs):
                if str(path) == '/proc/self/mountinfo':
                    if mounted:
                        return f'77 25 0:66 / {root}/runtime/doc rw - fuse.portal portal rw,user_id={os.getuid()}\n'
                    return ''
                if str(path) in ['/var/lib/dpkg/status', '/etc/os-release']:
                    return ''
                return original_read(path, *args, **kwargs)

            def remove(path, *args, **kwargs):
                if Path(path) == root / 'runtime' and mounted:
                    raise OSError(errno.ENOTCONN, 'Transport endpoint is not connected', 'doc')
                result = original_remove(path, *args, **kwargs)
                if Path(path) == root / 'home':
                    change_directory('after-home-removal')
                return result

            def unmount(args, **kwargs):
                nonlocal mounted
                self.assertEqual(args, ['/usr/bin/fusermount3', '-u', '--', str(root / 'runtime/doc')])
                self.assertTrue(mounted)
                mounted = False
                change_directory('unmount')
                return subprocess.CompletedProcess(args, 0, '', '')

            def fixtures(_):
                nonlocal mounted
                mounted = True
                (root / 'native-results.json').write_text(json.dumps({'status': 'passed'}))
                return {}

            with patch.object(runner, 'admit', return_value={'Xvfb': '/usr/bin/Xvfb', 'dbus-daemon': '/usr/bin/dbus-daemon'}), \
                    patch.object(runner.sys, 'platform', 'linux'), \
                    patch.object(runner.tempfile, 'mkdtemp', return_value=str(root)), \
                    patch.object(runner, 'generate', side_effect=fixtures), \
                    patch.object(runner, 'audio_configuration', return_value=('', [])), \
                    patch.object(runner, 'audio_evidence', return_value={'selection_observed': True}), \
                    patch.object(runner, 'line', side_effect=['12345', 'unix:path=' + str(root / 'runtime/bus')]), \
                    patch.object(runner, 'stop'), patch.object(runner.time, 'sleep'), \
                    patch.object(runner, 'processes', return_value={103: {'parent': 0, 'start': 1, 'rss': 1, 'name': 'WebKit-test'}}), \
                    patch.object(runner, 'installed_fusermount', create=True, return_value=('/usr/bin/fusermount3', '3.14.0')), \
                    patch.object(runner.subprocess, 'Popen', side_effect=children), \
                    patch.object(runner.subprocess, 'run', side_effect=unmount) as invoked, \
                    patch.object(Path, 'read_text', read), patch.object(Path, 'is_socket', return_value=True), \
                    patch.object(runner.shutil, 'rmtree', side_effect=remove), \
                    contextlib.redirect_stdout(io.StringIO()):
                result = runner.run(sys.executable, 'owned-test')
            report = json.loads((root / 'report.json').read_text())
            if mutation:
                self.assertFalse(report['cleanup'], report)
                self.assertEqual(result, 1)
                self.assertRegex(' '.join(report['cleanup_errors']), 'inode changed|identity or permissions')
                self.assertEqual((root / mutation[1] / 'retained.txt').read_text(), 'Replacement must not be removed')
                if mutation[0] == 'unmount':
                    self.assertFalse(report['mount_cleanup']['verified_absent'])
            else:
                self.assertTrue(report['cleanup'], report['cleanup_errors'])
                self.assertEqual(result, 0, report.get('error'))
                self.assertFalse((root / 'runtime').exists())
            invoked.assert_called_once()

    def test_mountinfo_decodes_owned_paths_and_keeps_mount_identity(self):
        root = Path('/tmp/owned space\\name\tline\nend')
        escaped = str(root).replace('\\', r'\134').replace(' ', r'\040').replace('\t', r'\011').replace('\n', r'\012')
        text = f'77 25 0:66 / {escaped}/runtime/doc rw,nosuid shared:8 - fuse.portal portal rw,user_id=1001\n'
        text += '88 25 0:67 / /tmp/other/runtime/doc rw - fuse.portal portal rw,user_id=1001\n'
        with patch.object(Path, 'read_text', return_value=text):
            mounts = runner.owned_mounts(root)
        self.assertEqual(len(mounts), 1)
        self.assertEqual(mounts[0]['mountpoint'], str(root / 'runtime/doc'))
        self.assertEqual(mounts[0]['id'], 77)
        self.assertEqual(mounts[0]['device'], '0:66')
        self.assertEqual(mounts[0]['type'], 'fuse.portal')

    def test_portal_cleanup_rejects_unknown_mounts_and_changed_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            (root / 'runtime').mkdir(mode=0o700)
            identities = {'': runner.directory_identity(root), 'runtime': runner.directory_identity(root / 'runtime')}
            mount = {'id': 77, 'device': '0:66', 'root': '/', 'mountpoint': str(root / 'runtime/doc'),
                     'type': 'fuse.portal', 'source': 'portal', 'options': ['rw', f'user_id={os.getuid()}']}
            invalid = [[{**mount, 'type': 'fuse.other'}], [{**mount, 'source': 'other'}],
                       [{**mount, 'root': '/elsewhere'}], [{**mount, 'options': ['user_id=999999']}],
                       [{**mount, 'mountpoint': str(root / 'data/unknown')}],
                       [mount, {**mount, 'id': 78, 'mountpoint': str(root / 'runtime/doc/nested')}]]
            with patch.object(runner.sys, 'platform', 'linux'), patch.object(runner, 'installed_fusermount') as tool, \
                    patch.object(runner.subprocess, 'run') as invoke:
                for mounts in invalid:
                    with self.subTest(mounts=mounts), patch.object(runner, 'owned_mounts', return_value=mounts):
                        with self.assertRaisesRegex(RuntimeError, 'mount'):
                            runner.cleanup_portal_mount(root, identities, {})
                (root / 'runtime').rename(root / 'previous-runtime')
                (root / 'runtime').mkdir(mode=0o700)
                with self.assertRaisesRegex(RuntimeError, 'inode changed'):
                    runner.cleanup_portal_mount(root, identities, {})
                tool.assert_not_called()
                invoke.assert_not_called()
            (root / 'runtime').rmdir()
            (root / 'runtime').symlink_to(root / 'previous-runtime', target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, 'identity or permissions'):
                runner.directory_identity(root / 'runtime')

    def test_portal_cleanup_keeps_tool_failure_mount_survival_and_races_failed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            (root / 'runtime').mkdir(mode=0o700)
            identities = {'': runner.directory_identity(root), 'runtime': runner.directory_identity(root / 'runtime')}
            mount = {'id': 77, 'device': '0:66', 'root': '/', 'mountpoint': str(root / 'runtime/doc'),
                     'type': 'fuse.portal', 'source': 'portal', 'options': [f'user_id={os.getuid()}']}
            with patch.object(runner.sys, 'platform', 'linux'), \
                    patch.object(runner, 'installed_fusermount', return_value=('/usr/bin/fusermount3', '3.14.0')):
                cases = [([[mount], [mount], [mount]], 1, 'normal unmount failed'),
                         ([[mount], [mount], [mount]], 0, 'Mount survived'),
                         ([[mount], [{**mount, 'id': 78}]], 0, 'identity changed')]
                for scans, code, message in cases:
                    evidence = {}
                    with self.subTest(message=message), patch.object(runner, 'owned_mounts', side_effect=scans), \
                            patch.object(runner.subprocess, 'run', return_value=subprocess.CompletedProcess([], code, '', 'busy')) as invoke:
                        with self.assertRaisesRegex(RuntimeError, message):
                            runner.cleanup_portal_mount(root, identities, evidence)
                        self.assertFalse(evidence['verified_absent'])
                        self.assertEqual(invoke.call_count, 0 if message == 'identity changed' else 1)
                with patch.object(runner, 'owned_mounts', return_value=[mount]), \
                        patch.object(runner.subprocess, 'run', side_effect=subprocess.TimeoutExpired('fusermount3', 10)):
                    with self.assertRaises(subprocess.TimeoutExpired):
                        runner.cleanup_portal_mount(root, identities, {})
                with patch.object(runner, 'owned_mounts', side_effect=[[mount], [], []]), \
                        patch.object(runner.subprocess, 'run') as invoke:
                    evidence = {}
                    runner.cleanup_portal_mount(root, identities, evidence)
                    self.assertTrue(evidence['verified_absent'])
                    self.assertEqual(evidence['action'], 'none')
                    invoke.assert_not_called()
                with patch.object(runner, 'owned_mounts', side_effect=[[], []]), \
                        patch.object(runner, 'installed_fusermount') as tool:
                    runner.cleanup_portal_mount(root, identities, {})
                    tool.assert_not_called()

    def test_fusermount_must_be_installed_packaged_and_resolves_usrmerge(self):
        from types import SimpleNamespace
        status = 'Package: fuse3\nStatus: install ok installed\nVersion: 3.14.0-5build1\nArchitecture: amd64\n'
        manifest = '/bin/fusermount3\n'

        def read(path, *args, **kwargs):
            return status if str(path) == '/var/lib/dpkg/status' else manifest

        def resolve(path, *args, **kwargs):
            return Path('/usr/bin/fusermount3') if str(path) == '/bin/fusermount3' else path

        with patch.object(Path, 'read_text', read), patch.object(Path, 'resolve', resolve), \
                patch.object(Path, 'is_file', return_value=True), \
                patch.object(Path, 'stat', return_value=SimpleNamespace(st_mode=0o100755, st_uid=0)), \
                patch.object(runner.os, 'access', return_value=True), \
                patch.object(runner.shutil, 'which', return_value='/usr/bin/fusermount3'):
            self.assertEqual(runner.installed_fusermount(), ('/usr/bin/fusermount3', '3.14.0-5build1'))
            manifest = '/usr/local/bin/fusermount3\n'
            with self.assertRaisesRegex(RuntimeError, 'verified installed'):
                runner.installed_fusermount()
            manifest = '/bin/fusermount3\n'
            with patch.object(Path, 'stat', return_value=SimpleNamespace(st_mode=0o100777, st_uid=0)):
                with self.assertRaisesRegex(RuntimeError, 'verified installed'):
                    runner.installed_fusermount()
            with patch.object(Path, 'stat', return_value=SimpleNamespace(st_mode=0o100755, st_uid=1001)):
                with self.assertRaisesRegex(RuntimeError, 'verified installed'):
                    runner.installed_fusermount()
            with patch.object(runner.shutil, 'which', return_value=None):
                with self.assertRaisesRegex(RuntimeError, 'already installed'):
                    runner.installed_fusermount()
            status = ''
            with self.assertRaisesRegex(RuntimeError, 'already installed'):
                runner.installed_fusermount()

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
