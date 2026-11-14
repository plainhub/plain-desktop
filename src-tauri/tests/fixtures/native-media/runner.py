import json
import os
import re
from pathlib import Path
import secrets
import selectors
import shutil
import signal
import stat
import struct
import subprocess
import sys
import sysconfig
import tempfile
import time

sys.dont_write_bytecode = True
from generate import generate


def legacy_diagnostic_log(log):
    state = r'(?:NULL|READY|PAUSED|PLAYING|VOID_PENDING)'
    media_time = r'\{[-\d./ =]+\}'
    gst_time = r'-?\d{1,3}:\d{2}:\d{2}\.\d{1,9}'
    patterns = [
        r'(?:Async: |Failure: |No preroll: )?State: ' + state + ', pending: ' + state,
        r'Old state: ' + state + ', new state: ' + state + r' \(requested: ' + state + r'\)',
        r'Playback state changed from ' + state + ' to ' + state + r'\. Notifying the media player client',
        r'Network State Changed from (?:Empty|Idle|Loading|Loaded|FormatError|NetworkError|DecodeError) to (?:Empty|Idle|Loading|Loaded|FormatError|NetworkError|DecodeError)',
        r'Ready State Changed from Have(?:Nothing|Metadata|CurrentData|FutureData|EnoughData) to Have(?:Nothing|Metadata|CurrentData|FutureData|EnoughData)',
        r'Processing playback request|Playback ended|Flushing video sample |Unhandled GStreamer message type: new-clock',
        r'Using legacy playbin element: (?:true|false)',
        r'\[Buffering\] Complete\.|Async: \[Buffering\] still buffering, so force HaveCurrentData\.',
        r'\[Buffering\] mode: GST_BUFFERING_(?:STREAM|DOWNLOAD|TIMESHIFT|LIVE), status: \d+\.\d+%',
        r'\[Buffering\] Updated maxTimeLoaded: ' + media_time,
        r'\[Buffering\] Pausing (?:stream for buffering\.|stream because of zero playback rate\.|pipeline, resetting buffering level to 0 and forcing m_isBuffering true before seeking on stream mode)',
        r'\[Buffering\] Restarting playback \(because of buffering or resuming from zero playback rate\)',
        r'\[Seek\] (?:seek attempt to|seeking to|seeked to|committing pending seek to) ' + media_time + r'(?: failed)?',
        r'\[Seek\] Performing actual seek to ' + gst_time + r' \(endTime: ' + gst_time + r'\) at rate -?\d+\.\d+',
        r'\[Seek\] (?:Already at requested position\. Aborting\.|Live stream seek unhandled|m_player is nullptr|A seek is pending already, letting it finish|reset pipeline)',
        r'\[Seek\] cannot seek, current state change is (?:FAILURE|SUCCESS|ASYNC|NO_PREROLL)',
    ]
    sink_patterns = [
        r'(?:rendering|prerolling) object 0x[0-9a-f]+',
        r'(?:PAUSED to PLAYING, don\x27t need preroll|PAUSED to READY, don\x27t need_preroll|PLAYING to PAUSED, we are not prerolled)',
        r'(?:do_sync returned flushing|flushing out data thread, need preroll to TRUE|got preroll lock|preroll failed(?:: flushing)?|preroll interrupted because of flush|unschedule clock|waiting in preroll for flush or PLAYING)',
        r'have_preroll: [01], EOS: [01] => needs preroll: [01]',
        r'clock returned \d+, jitter ' + gst_time,
        r'preroll buffer ' + gst_time,
        r'using clock and base time ' + gst_time,
        r'possibly waiting for clock to reach ' + gst_time + ', adjusted ' + gst_time,
    ]
    safe = []
    for line in log.splitlines():
        if len(line) > 2048:
            continue
        match = re.fullmatch(r'\s*(\d{1,3}:\d{2}:\d{2}\.\d{1,9})\s+\d+\s+\S+\s+(?:DEBUG|LOG|INFO|WARN|ERROR)\s+'
            r'(webkitmediaplayer|basesink)\s+[^<>\r\n]*<([^<>\r\n]+)>\s+([^\r\n]*)', line)
        if not match:
            continue
        timestamp, category, source, message = match.groups()
        if not any(re.fullmatch(pattern, message) for pattern in (patterns if category == 'webkitmediaplayer' else sink_patterns)):
            continue
        if not re.fullmatch(r'regular-plain-native-(?:video-webm|audio-wav|small-video-webm|control-wav)-\d{1,3}', source):
            source = 'fake-audio-sink' if category == 'basesink' and source == 'fake-audio-sink' else 'other-owned-pipeline'
        safe.append(f'{timestamp} 0 0x0 DEBUG {category} diagnostic:0:filtered:<{source}> {message}')
    return '\n'.join(safe)


def pipeline_evidence(log):
    groups = {}
    omitted = 0
    for line in log.splitlines():
        if not re.match(r'^\s*\d+:\d{2}:\d{2}\.\d+', line):
            continue
        if not re.match(r'^\s*\d+:\d{2}:\d{2}\.\d+\s+\d+\s+\S+\s+\S+\s+(?:webkitmediaplayer|basesink)\s', line):
            continue
        if not re.search(r'\[Seek\]|\[Buffering\]|State Changed|Processing playback|preroll|'
                         r'Async: State|State: |Playback ended|clock|segment|flush|Using legacy playbin element', line, re.I):
            continue
        pipeline = re.search(r'<(regular-plain-native-(?:video-webm|audio-wav|small-video-webm|control-wav)-\d{1,3})>', line)
        key = pipeline.group(1) if pipeline else 'other-owned-pipeline'
        if key not in groups:
            if len(groups) == 12:
                omitted += 1
                continue
            groups[key] = {'matched_lines': 0, 'retained_lines': [], 'seek_lines': []}
        group = groups[key]
        backend = re.search(r'Using legacy playbin element: (true|false)$', line)
        if backend:
            group['legacy_playbin'] = backend[1] == 'true'
        group['matched_lines'] += 1
        if len(group['retained_lines']) == 32:
            del group['retained_lines'][8]
        safe = redacted_log(line)[:480]
        group['retained_lines'].append(safe)
        if '[Seek]' in line and len(group['seek_lines']) < 16:
            group['seek_lines'].append(safe)
    return {'pipelines': groups, 'omitted_pipeline_lines': omitted,
            'method': 'Per pipeline first 8/latest 24 state records plus first 16 seek records; URLs redacted; at most 12 pipelines'}



# WebKit 2.52.6 WebKitWebSourceGStreamer.cpp: 628-664, 1029-1074, 1200, 1248-1270.
def http_header_fields(value, kind):
    prefix = kind + '-headers, '
    if not value.startswith(prefix) or not value.endswith(';'):
        return None
    cursor, end, fields = len(prefix), len(value) - 1, {}
    while cursor < end:
        match = re.match(r'([A-Za-z0-9-]+)=\((string|guint64)\)', value[cursor:])
        if not match:
            return None
        name, scalar = match.groups()
        cursor += match.end()
        quoted = cursor < end and value[cursor] == '"'
        if quoted:
            cursor += 1
        chars, closed = [], not quoted
        while cursor < end:
            char = value[cursor]
            if char == '\\':
                cursor += 1
                if cursor >= end or value[cursor].isalnum() or ord(value[cursor]) < 32:
                    return None
                chars.append(value[cursor])
                cursor += 1
            elif quoted and char == '"':
                cursor += 1
                closed = True
                break
            elif not quoted and char == ',':
                break
            elif char in '\r\n' or ord(char) < 32:
                return None
            else:
                chars.append(char)
                cursor += 1
        if not closed:
            return None
        if cursor < end:
            if value[cursor:cursor + 2] != ', ':
                return None
            cursor += 2
            if cursor == end:
                return None
        name = name.lower()
        if name in fields:
            return None
        fields[name] = (scalar, ''.join(chars))
    return fields


def http_evidence(lines):
    sources, requests, events = {}, {}, []
    omitted, unobserved = 0, 0
    current = {}
    for line in lines:
        if len(line) > 16384:
            unobserved += 1
            continue
        match = re.fullmatch(r'\s*(\d{1,3}:\d{2}:\d{2}\.\d{1,9})\s+(\d+)\s+\S+\s+(?:DEBUG|LOG|INFO|WARN|ERROR)\s+'
                             r'webkitwebsrc\s+[^<>\r\n]*<([^<>\r\n]{1,128})>\s+([^\r\n]*)\n?', line)
        if not match:
            continue
        timestamp, pid, source, message = match.groups()
        source = (pid, source)
        if source not in sources:
            if len(sources) >= 16:
                omitted += 1
                continue
            sources[source] = len(sources) + 1
        source = sources[source]
        posting = re.fullmatch(r'Posting task to request R(\d{1,10}) .* requestedPosition=(\d{1,20}) stopPosition=(\d{1,20})', message)
        numbered = re.match(r'R(\d{1,10}): ', message)
        if posting or numbered:
            number = int((posting or numbered).group(1))
        else:
            number = current.get(source)
        if number is None:
            unobserved += 1
            continue
        if numbered:
            message = message[numbered.end():]
        status = re.fullmatch(r'Received response: ([1-5]\d{2})', message)
        data = re.fullmatch(r'Have (\d{1,10}) bytes of data', message)
        finished = re.fullmatch(r'Load finished\. Read position: (\d{1,20})', message)
        range_value = re.fullmatch(r'Range request: (bytes=\d{1,20}-\d{0,20})', message)
        seek = re.fullmatch(r'Seek segment: \((\d{1,20})-(\d{1,20})\) Position previous to seek: (\d{1,20})', message)
        headers = re.fullmatch(r'(Request|Response) headers going downstream: (.*)', message)
        canceled = message.startswith('Request canceled: ')
        stopped = message in ('stopping download', 'Stopped WebKitWebSrc', 'Unlock', 'Unlock stop')
        if not (posting or status or data or finished or range_value or seek or headers or canceled or stopped):
            unobserved += 1
            continue
        if posting or numbered:
            current[source] = number
        key = (source, number)
        if key not in requests:
            if len(requests) >= 64:
                omitted += 1
                continue
            requests[key] = {'source': source, 'request': number, 'first_time': timestamp, 'received_bytes': None,
                             'data_records': 0, 'request_headers': 'unobserved', 'response_headers': 'unobserved'}
        request = requests[key]
        request['last_time'] = timestamp
        if posting:
            request.update(requested_position=int(posting[2]), stop_position=int(posting[3]))
            continue
        if status:
            request['status'] = int(status[1])
        elif data:
            request['received_bytes'] = (request['received_bytes'] or 0) + int(data[1])
            request['data_records'] += 1
        elif finished:
            request.update(finished=True, read_position=int(finished[1]))
        elif canceled:
            request['canceled'] = True
        elif range_value:
            request['range'] = range_value[1]
        elif headers:
            kind = headers[1].lower()
            parsed = http_header_fields(headers[2], kind)
            if parsed is None:
                unobserved += 1
                continue
            safe = {}
            patterns = {'range': r'bytes=\d{1,20}-\d{0,20}',
                        'content-length': r'\d{1,20}',
                        'content-range': r'bytes (?:\d{1,20}-\d{1,20}|\*)/\d{1,20}',
                        'accept-ranges': r'bytes|none',
                        'content-type': r'audio/(?:wav|x-wav)|video/webm|application/octet-stream'}
            for name, (scalar, value) in parsed.items():
                if name not in patterns or (kind == 'request') != (name == 'range'):
                    continue
                if (scalar == 'string' or name == 'content-length') and re.fullmatch(patterns[name], value):
                    safe[name] = int(value) if name == 'content-length' else value
                else:
                    unobserved += 1
            request[kind + '_headers'] = safe
        elif seek or stopped:
            if len(events) < 128:
                event = {'source': source, 'request': number, 'time': timestamp, 'event': 'seek' if seek else message}
                if seek:
                    event.update(start=int(seek[1]), stop=int(seek[2]), previous=int(seek[3]))
                events.append(event)
            else:
                omitted += 1
    return {'method': 'WebKit webkitwebsrc debug records; strict field allowlist; no raw headers or URLs',
            'observed': bool(events) or any(row['data_records'] or
                any(key in row for key in ('status', 'requested_position', 'range', 'finished', 'canceled')) or
                isinstance(row['request_headers'], dict) or isinstance(row['response_headers'], dict)
                for row in requests.values()), 'requests': list(requests.values()), 'events': events,
            'omitted_records': omitted, 'unobserved_records': unobserved}


# WebKit 2.52.6 MediaPlayerPrivateGStreamer.cpp:3036 (updateStates).
def buffering_evidence(lines):
    events, total = [], 0
    for line in lines:
        if len(line) > 2048:
            continue
        match = re.fullmatch(r'\s*(\d+:\d{2}:\d{2}\.\d+)\s+\d+\s+\S+\s+TRACE\s+webkitmediaplayer\s+'
            r'[^<>\r\n]*<(regular-plain-native-(?:video-webm|audio-wav|small-video-webm|control-wav)-\d{1,3})>\s+'
            r'\[Buffering\] Delaying m_isBuffering (true|false) --> (true|false) to force the proper change '
            r'from not buffering to buffering when the async state change completes\.', line)
        if match:
            total += 1
            if len(events) < 32:
                events.append({'time': match[1], 'pipeline': match[2], 'previous': match[3] == 'true',
                               'current': match[4] == 'true'})
    return {'method': 'Exact WebKit updateStates async buffering deferral trace',
            'observed': total > 0, 'matched_records': total, 'events': events}

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


def directory_identity(path):
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o777 != 0o700:
        raise RuntimeError('Owned cleanup directory identity or permissions changed')
    return info.st_dev, info.st_ino


def verify_directory_identities(root, identities):
    for name, expected in identities.items():
        if directory_identity(root / name) != expected:
            raise RuntimeError('Owned cleanup directory inode changed')


def owned_mounts(root):
    result = []
    for line in Path('/proc/self/mountinfo').read_text().splitlines():
        before, after = line.split(' - ', 1)
        fields, filesystem = before.split(), after.split()
        mountpoint = Path(re.sub(r'\\(040|011|012|134)', lambda match: chr(int(match[1], 8)), fields[4]))
        if mountpoint == root or root in mountpoint.parents:
            result.append({'id': int(fields[0]), 'device': fields[2], 'root': fields[3],
                           'mountpoint': str(mountpoint), 'type': filesystem[0],
                           'source': filesystem[1], 'options': filesystem[2].split(',')})
    return result


def installed_fusermount():
    version, architecture = None, None
    for block in Path('/var/lib/dpkg/status').read_text().split('\n\n'):
        fields = dict(line.split(': ', 1) for line in block.splitlines() if ': ' in line and not line.startswith(' '))
        if fields.get('Package') == 'fuse3' and fields.get('Status') == 'install ok installed':
            version, architecture = fields.get('Version'), fields.get('Architecture')
    found = shutil.which('fusermount3')
    if not version or not found:
        raise RuntimeError('Owned portal cleanup requires an already installed fuse3/fusermount3')
    executable = Path(found).resolve(strict=True)
    manifests = [Path('/var/lib/dpkg/info/fuse3.list'), Path('/var/lib/dpkg/info/fuse3:' + str(architecture) + '.list')]
    packaged = any(executable == Path(item).resolve() for manifest in manifests if manifest.is_file()
                   for item in manifest.read_text().splitlines() if item.endswith('/fusermount3'))
    info = executable.stat()
    if not packaged or not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022 or not os.access(executable, os.X_OK):
        raise RuntimeError('Owned portal cleanup executable is not a verified installed fuse3 file')
    return str(executable), version


def cleanup_portal_mount(root, identities, evidence):
    if sys.platform != 'linux':
        raise RuntimeError('Owned portal cleanup requires the admitted Linux runner')
    verify_directory_identities(root, identities)
    mounts = owned_mounts(root)
    evidence.update(before=mounts, action='none', verified_absent=False)
    if mounts:
        target = root / 'runtime/doc'
        if 'runtime' not in identities or len(mounts) != 1:
            raise RuntimeError('Unknown or nested mount prevents owned cleanup')
        mount = mounts[0]
        if (mount['mountpoint'] != str(target) or mount['root'] != '/' or mount['type'] != 'fuse.portal'
                or mount['source'] != 'portal'
                or [item for item in mount['options'] if item.startswith('user_id=')] != [f'user_id={os.getuid()}']):
            raise RuntimeError('Portal mount path, filesystem or UID does not match owned cleanup')
        executable, version = installed_fusermount()
        current = owned_mounts(root)
        if current and current != mounts:
            raise RuntimeError('Portal mount identity changed before unmount')
        if current:
            verify_directory_identities(root, identities)
            evidence.update(action='normal-unmount', tool=executable, package='fuse3', version=version)
            try:
                result = subprocess.run([executable, '-u', '--', str(target)], capture_output=True, text=True,
                                        timeout=10, env={'PATH': '/usr/bin:/bin', 'LANG': 'C'})
            finally:
                evidence['after'] = owned_mounts(root)
            evidence['returncode'] = result.returncode
            if result.returncode:
                raise RuntimeError('Owned portal normal unmount failed: ' + redacted_log(result.stderr)[:500])
    if 'after' not in evidence:
        evidence['after'] = owned_mounts(root)
    if evidence['after']:
        raise RuntimeError('Mount survived owned portal cleanup')
    verify_directory_identities(root, identities)
    evidence['verified_absent'] = True


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
    root = Path(tempfile.mkdtemp(prefix='plain-native-media-')).resolve(strict=True)
    identities = {'': directory_identity(root)}
    children, logs = [], []
    report = {'status': 'failed', 'root': str(root), 'tools': tools, 'peak_tree_rss_bytes': 0,
              'sample_interval_seconds': .05, 'memory_method': 'sum RSS of observed owned process tree; shared pages may be counted twice',
              'processes': {}, 'cleanup': False, 'memory_sample_count': 0, 'first_tree_rss_bytes': None}
    error = None
    try:
        if owned_mounts(root):
            raise RuntimeError('Fresh owned root already contains a mount')
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
            identities[name] = directory_identity(root / name)
        allowed = ['PATH', 'LD_LIBRARY_PATH', 'LANG', 'LC_ALL', 'TZ', 'GITHUB_ACTIONS', 'GITHUB_REPOSITORY',
                   'GITHUB_WORKSPACE', 'GITHUB_RUN_ID', 'GITHUB_SHA', 'RUNNER_ENVIRONMENT', 'RUNNER_OS',
                   'WEBKIT_GST_USE_PLAYBIN3']
        env = {key: os.environ[key] for key in allowed if key in os.environ}
        env.update(HOME=str(root / 'home'), XDG_RUNTIME_DIR=str(root / 'runtime'), XDG_CONFIG_HOME=str(root / 'config'),
                   XDG_CACHE_HOME=str(root / 'cache'), XDG_DATA_HOME=str(root / 'data'), GDK_BACKEND='x11',
                   GIO_USE_VFS='local', GTK_USE_PORTAL='0', PULSE_SERVER='unix:' + str(root / 'no-audio'),
                   PLAIN_NATIVE_MEDIA_ROOT=str(root), PLAIN_NATIVE_MEDIA_CHILD='1',
                   GST_PLUGIN_FEATURE_RANK=audio_rank, WEBKIT_GST_MAX_NUMBER_OF_AUDIO_OUTPUT_CHANNELS='2',
                   GST_DEBUG='autodetect:5,basesink:5,webkitmediaplayer:7,webkitwebsrc:6', GST_DEBUG_NO_COLOR='1')
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
        native_log = (root / 'native.log').read_text()
        report['audio_environment'].update(audio_evidence(legacy_diagnostic_log(native_log)))
        if (root / 'native-results.json').is_file():
            report['native'] = json.loads((root / 'native-results.json').read_text())
        pipelines = pipeline_evidence(legacy_diagnostic_log(native_log))['pipelines']
        backends = {name: value['legacy_playbin'] for name, value in pipelines.items() if 'legacy_playbin' in value}
        report['media_backend'] = {'expected': 'playbin3', 'legacy_playbin_by_pipeline': backends}
        if child.returncode != 0 or report.get('native', {}).get('status') != 'passed':
            raise RuntimeError('Native acceptance failed or lacks a complete result; inspect native.log and structured results')
        if any(backends.values()) or not all(
                any(name.startswith('regular-plain-native-' + fixture + '-') for name in backends)
                for fixture in ['video-webm', 'audio-wav', 'small-video-webm', 'control-wav']):
            raise RuntimeError('WebKit did not confirm playbin3 for every owned media fixture')
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
                report['mount_cleanup'] = {}
                cleanup_portal_mount(root, identities, report['mount_cleanup'])
                for name in ['home', 'runtime', 'config', 'cache', 'data']:
                    verify_directory_identities(root, identities)
                    if owned_mounts(root):
                        raise RuntimeError('Unexpected mount prevents owned directory removal')
                    path = root / name
                    if name in identities:
                        shutil.rmtree(path)
                        del identities[name]
                    elif path.exists() or path.is_symlink():
                        raise RuntimeError('Owned cleanup directory lacks its creation identity')
                verify_directory_identities(root, identities)
                for name in ['video.webm', 'small-video.webm', 'audio.wav', 'control.wav', 'image.png', 'authority']:
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
        if (root / 'native.log').is_file():
            native_log = (root / 'native.log').read_text()
            report['pipeline_diagnostics'] = pipeline_evidence(legacy_diagnostic_log(native_log))
            report['http_diagnostics'] = http_evidence(native_log.splitlines())
            report['buffering_deferrals'] = buffering_evidence(native_log.splitlines())
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print('NATIVE_MEDIA_REPORT=' + json.dumps(report), flush=True)
        for name in ['native.log', 'xvfb.log', 'dbus.log']:
            path = root / name
            if path.exists():
                if not failures:
                    path.unlink()
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(run(sys.argv[1], sys.argv[2]))
