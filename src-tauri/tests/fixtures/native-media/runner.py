import json
import os
import re
from pathlib import Path
import secrets
import selectors
import shutil
import signal
import struct
import subprocess
import sys
import sysconfig
import tempfile
import time

sys.dont_write_bytecode = True
from generate import generate


def admit():
    expected = {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted',
                'RUNNER_OS': 'Linux', 'GITHUB_REPOSITORY': 'plainhub/plain-desktop'}
    if sys.platform != 'linux' or os.getuid() == 0 or any(os.environ.get(k) != v for k, v in expected.items()):
        raise RuntimeError('NOT RUN: native GUI requires the dedicated GitHub-hosted Linux repository runner')
    workspace = Path(os.environ['GITHUB_WORKSPACE']).resolve(strict=True)
    Path(__file__).resolve().relative_to(workspace)
    tools = {}
    for name in ['Xvfb', 'dbus-daemon']:
        path = shutil.which(name)
        if not path:
            raise RuntimeError('NOT RUN: existing runner executable is missing: ' + name)
        tools[name] = str(Path(path).resolve(strict=True))
    return tools


def line(process):
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        if not selector.select(15):
            raise RuntimeError('Owned GUI prerequisite did not publish readiness')
    result = process.stdout.readline().strip()
    if not result or process.poll() is not None:
        raise RuntimeError('Owned GUI prerequisite exited before readiness')
    return result


def authority(path, number, cookie):
    fields = [b'', number.encode(), b'MIT-MAGIC-COOKIE-1', cookie]
    path.write_bytes(struct.pack('>H', 65535) + b''.join(struct.pack('>H', len(x)) + x for x in fields))
    path.chmod(0o600)


def processes():
    result = {}
    for path in Path('/proc').iterdir():
        if not path.name.isdecimal():
            continue
        try:
            fields = (path / 'stat').read_text().rsplit(')', 1)[1].split()
            result[int(path.name)] = {'parent': int(fields[1]), 'start': int(fields[19]),
                                     'rss': int(fields[21]) * os.sysconf('SC_PAGE_SIZE'),
                                     'name': (path / 'comm').read_text().strip()}
        except (FileNotFoundError, ProcessLookupError):
            pass
    return result


def stop(process):
    # Only sessions created by this supervisor may be terminated.
    if process.poll() is None:
        if os.getpgid(process.pid) != process.pid:
            raise RuntimeError('Owned child process-group identity changed')
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(10)
    try:
        os.killpg(process.pid, 0)
    except ProcessLookupError:
        return
    os.killpg(process.pid, signal.SIGTERM)
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            return
        time.sleep(.05)
    raise RuntimeError('Owned GUI process group survived cleanup')


def audio_configuration():
    multiarch = sysconfig.get_config_var('MULTIARCH')
    if not multiarch:
        raise RuntimeError('NOT RUN: installed GStreamer architecture cannot be verified')
    plugins = Path('/usr/lib') / multiarch / 'gstreamer-1.0'
    required = [plugins / 'libgstcoreelements.so', plugins / 'libgstautodetect.so']
    if not all(path.is_file() for path in required):
        raise RuntimeError('NOT RUN: installed coreelements/autoaudiosink plugins are unavailable')
    sinks = ['pulsesink', 'alsasink', 'osssink', 'oss4sink', 'jackaudiosink', 'pipewiresink']
    providers = ['pulsedeviceprovider', 'alsadeviceprovider', 'ossdeviceprovider', 'pipewiredeviceprovider']
    return ','.join(name + ':0' for name in sinks + providers), [str(path) for path in required]


def audio_evidence(log):
    rendered = sum('fake-audio-sink' in line and 'rendering object' in line for line in log.splitlines())
    return {'method': 'GStreamer basesink debug records for rendered buffers',
            'expected_sink': 'coreelements fakesink selected by autoaudiosink without hardware candidates',
            'observed_sink': 'GstFakeSink/fake-audio-sink' if rendered else None,
            'rendered_fake_sink_buffers': rendered, 'selection_observed': rendered > 0,
            'scope': 'decode and playback timing only; no physical speaker output'}


def redacted_log(log):
    return re.sub(r"https?://[^\s\"'<>]+", '[HTTP resource URL omitted]', log)


def run(binary, test):
    tools = admit()
    root = Path(tempfile.mkdtemp(prefix='plain-native-media-'))
    children, logs = [], []
    report = {'status': 'failed', 'root': str(root), 'tools': tools, 'peak_tree_rss_bytes': 0,
              'sample_interval_seconds': .05, 'memory_method': 'sum RSS of observed owned process tree; shared pages may be counted twice',
              'processes': {}, 'cleanup': False, 'memory_sample_count': 0, 'first_tree_rss_bytes': None}
    error = None
    try:
        packages = ['xvfb', 'dbus-daemon', 'python3', 'libwebkit2gtk-4.1-0', 'libjavascriptcoregtk-4.1-0',
                    'gstreamer1.0-plugins-good', 'gstreamer1.0-plugins-base', 'libgstreamer1.0-0', 'gstreamer1.0-libav']
        report['packages'] = {}
        for block in Path('/var/lib/dpkg/status').read_text().split('\n\n'):
            fields = dict(line.split(': ', 1) for line in block.splitlines() if ': ' in line and not line.startswith(' '))
            if fields.get('Package') in packages:
                report['packages'][fields['Package']] = {key: fields.get(key) for key in ['Version', 'Status', 'Architecture']}
        report['os_release'] = Path('/etc/os-release').read_text()
        audio_rank, audio_plugins = audio_configuration()
        report['audio_environment'] = {'feature_ranks': audio_rank, 'installed_plugins': audio_plugins}
        report['fixtures'] = generate(root)
        for name in ['home', 'runtime', 'config', 'cache', 'data']:
            (root / name).mkdir(mode=0o700)
        allowed = ['PATH', 'LD_LIBRARY_PATH', 'LANG', 'LC_ALL', 'TZ', 'GITHUB_ACTIONS', 'GITHUB_REPOSITORY',
                   'GITHUB_WORKSPACE', 'GITHUB_RUN_ID', 'GITHUB_SHA', 'RUNNER_ENVIRONMENT', 'RUNNER_OS']
        env = {key: os.environ[key] for key in allowed if key in os.environ}
        env.update(HOME=str(root / 'home'), XDG_RUNTIME_DIR=str(root / 'runtime'), XDG_CONFIG_HOME=str(root / 'config'),
                   XDG_CACHE_HOME=str(root / 'cache'), XDG_DATA_HOME=str(root / 'data'), GDK_BACKEND='x11',
                   GIO_USE_VFS='local', GTK_USE_PORTAL='0', PULSE_SERVER='unix:' + str(root / 'no-audio'),
                   PLAIN_NATIVE_MEDIA_ROOT=str(root), PLAIN_NATIVE_MEDIA_CHILD='1',
                   GST_PLUGIN_FEATURE_RANK=audio_rank, WEBKIT_GST_MAX_NUMBER_OF_AUDIO_OUTPUT_CHANNELS='2',
                   GST_DEBUG='autodetect:5,basesink:5', GST_DEBUG_NO_COLOR='1')
        cookie = secrets.token_bytes(16)
        auth = root / 'authority'
        authority(auth, '', cookie)
        env['XAUTHORITY'] = str(auth)

        def launch(args, name, pipe=False):
            log = (root / (name + '.log')).open('w')
            logs.append(log)
            child = subprocess.Popen(args, env=env, stdout=subprocess.PIPE if pipe else log,
                                     stderr=log, text=True, start_new_session=True)
            children.append(child)
            return child

        x = launch([tools['Xvfb'], '-displayfd', '1', '-screen', '0', '1024x768x24', '-nolisten', 'tcp', '-auth', str(auth)], 'xvfb', True)
        display = line(x)
        if not display.isdecimal() or not Path('/tmp/.X11-unix/X' + display).is_socket():
            raise RuntimeError('Owned Xvfb display is invalid')
        authority(auth, display, cookie)
        env['DISPLAY'] = ':' + display
        bus = launch([tools['dbus-daemon'], '--session', '--nofork', '--print-address=1',
                      '--address=unix:path=' + str(root / 'runtime/bus')], 'dbus', True)
        address = line(bus)
        if not address.startswith('unix:path=' + str(root / 'runtime/bus')):
            raise RuntimeError('Owned DBus socket identity changed')
        env['DBUS_SESSION_BUS_ADDRESS'] = address
        (root / 'admission.json').write_text(json.dumps({'parent': os.getpid(), 'display': env['DISPLAY'],
            'bus': address, 'root': str(root), 'binary': str(Path(binary).resolve(strict=True)),
            'xvfb': x.pid, 'dbus': bus.pid}))
        child = launch([binary, '--exact', test, '--ignored', '--nocapture', '--test-threads=1'], 'native')
        owned = {}
        deadline = time.monotonic() + 240
        while child.poll() is None:
            if time.monotonic() >= deadline:
                raise RuntimeError('Native media watchdog expired')
            if x.poll() is not None or bus.poll() is not None:
                raise RuntimeError('Owned display/session bus exited during native acceptance')
            snapshot = processes()
            selected = {child.pid}
            for _ in range(10):
                expanded = selected | {pid for pid, value in snapshot.items() if value['parent'] in selected}
                if expanded == selected:
                    break
                selected = expanded
            for pid in selected:
                if pid in snapshot:
                    owned[pid] = snapshot[pid]['start']
                    report['processes'][str(pid)] = snapshot[pid]['name']
            rss = sum(v['rss'] for pid, v in snapshot.items() if owned.get(pid) == v['start'])
            if report['first_tree_rss_bytes'] is None:
                report['first_tree_rss_bytes'] = rss
            report['memory_sample_count'] += 1
            report['peak_tree_rss_bytes'] = max(report['peak_tree_rss_bytes'], rss)
            time.sleep(.05)
        report['native_exit'] = child.returncode
        report['audio_environment'].update(audio_evidence((root / 'native.log').read_text()))
        if (root / 'native-results.json').is_file():
            report['native'] = json.loads((root / 'native-results.json').read_text())
        if child.returncode != 0 or report.get('native', {}).get('status') != 'passed':
            raise RuntimeError('Native acceptance failed or lacks a complete result; inspect native.log and structured results')
        if not report['audio_environment']['selection_observed']:
            raise RuntimeError('Audio environment verification failed: no rendered fake-audio-sink buffer was observed')
        if not any('WebKit' in name for name in report['processes'].values()):
            raise RuntimeError('Native WebKit subprocess memory ownership was not observed')
        report['status'] = 'passed'
    except BaseException as exc:
        error = str(exc)
    finally:
        failures = []
        for child in reversed(children):
            try:
                stop(child)
            except BaseException as exc:
                failures.append(str(exc))
        for log in logs:
            log.close()
        # Retain only evidence; disposable media and application state are removed after process cleanup.
        if not failures:
            try:
                for name in ['home', 'runtime', 'config', 'cache', 'data']:
                    path = root / name
                    if path.exists():
                        shutil.rmtree(path)
                for name in ['video.webm', 'small-video.webm', 'audio.wav', 'control.wav', 'authority']:
                    (root / name).unlink(missing_ok=True)
                if 'display' in locals() and Path('/tmp/.X11-unix/X' + display).exists():
                    raise RuntimeError('Owned Xvfb socket survived cleanup')
            except BaseException as exc:
                failures.append(str(exc))
        report['cleanup_errors'] = failures
        report['cleanup'] = not failures
        if failures:
            report['status'] = 'failed'
        if error:
            report['error'] = error
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print('NATIVE_MEDIA_REPORT=' + json.dumps(report), flush=True)
        for name in ['native.log', 'xvfb.log', 'dbus.log']:
            path = root / name
            if path.exists():
                print('=== ' + name + '\n' + redacted_log(path.read_text()[-16000:]), flush=True)
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(run(sys.argv[1], sys.argv[2]))
