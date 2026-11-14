(() => {
if (window.__plainNativeMediaTest) return;
window.__plainNativeMediaTest = true;
const urls = FIXTURE_URLS;
let sequence = 0;
let packetSequence = 0;
let pending = 0;
let acknowledged = 0;
let transportFailure = null;
let delivery = Promise.resolve();
function errorText(error) {
  if (error instanceof DOMException) return error.name;
  return String(error).replace(/https?:\/\/[^\s"'<>]+/g, '[HTTP resource]');
}
function transmit(kind, payload) {
  if (transportFailure) return Promise.resolve(false);
  const packet = { sequence: ++packetSequence, kind, payload };
  if (packetSequence > 512 || pending >= 256 || new TextEncoder().encode(JSON.stringify(packet)).length > 16384) {
    transportFailure = 'Report transport bound exceeded';
    return Promise.resolve(false);
  }
  pending++;
  delivery = delivery.then(async () => {
    try {
      if (transportFailure) return false;
      const ack = await within(window.__TAURI_INTERNALS__.invoke('native_media_report', { packet }), 'Report acknowledgement', 12000);
      check(ack === packet.sequence, 'Report acknowledgement sequence differs');
      acknowledged = ack;
      return true;
    } catch (error) { transportFailure = errorText(error); return false; }
    finally { pending--; }
  });
  return delivery;
}
window.__plainTransportState = () => ({ failure: transportFailure, pending, sequence: packetSequence, acknowledged });
async function emit(stage) {
  await delivery;
  check(!transportFailure && pending === 0, 'Report transport did not become idle');
  check(await transmit('event', {
    stage, sequence: ++sequence, origin: window.origin, href: location.href,
    transport: { pending, failure: transportFailure, prior_sequence: packetSequence }
  }), 'Stage acknowledgement failed');
}
function diagnostic(stage, detail = {}) {
  return transmit('diagnostic', {
    stage, detail, origin: window.origin,
    document_url: location.href === 'tauri://localhost/native-media-test.html' ? location.href : '[Unexpected document URL omitted]'
  });
}
function mediaStage(result, stage) {
  result.stages.push(stage);
  diagnostic('media-stage', { fixture: result.fixture, route: result.route, stage });
}
function check(value, message) { if (!value) throw Error(message); }
function delay(ms) { return new Promise(resolve => setTimeout(resolve, ms)); }
function event(media, name, action) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => finish(Error('Timeout: ' + name)), 12000);
    const ok = () => finish();
    const bad = () => finish(Error(media.tagName === 'IMG' ? 'Image load failed' : 'Media error code ' + media.error?.code));
    function finish(error) {
      clearTimeout(timer);
      media.removeEventListener(name, ok);
      media.removeEventListener('error', bad);
      error ? reject(error) : resolve();
    }
    media.addEventListener(name, ok, { once: true });
    media.addEventListener('error', bad, { once: true });
    try { action(); } catch (error) { finish(error); }
  });
}
async function within(promise, name, timeout = 12000) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(Error('Timeout: ' + name)), timeout);
    })]);
  } finally { clearTimeout(timer); }
}
async function waitFor(predicate, message, timeout = 12000) {
  const deadline = performance.now() + timeout;
  while (!predicate() && performance.now() < deadline) await delay(10);
  check(predicate(), message);
}
function mediaState(media, original = '') {
  const ranges = value => ({ count: value.length, ranges: Array.from({ length: Math.min(value.length, 4) },
    (_, i) => [value.start(i), value.end(i)]) });
  return { network_state: media.networkState, ready_state: media.readyState, paused: media.paused,
    seeking: media.seeking, ended: media.ended, current_time: media.currentTime,
    current_src: media.currentSrc === '' ? 'empty' : media.currentSrc === original ? 'original' : 'other',
    has_src: media.hasAttribute('src'), has_src_object: media.srcObject !== null,
    source_children: media.querySelectorAll('source').length,
    buffered: ranges(media.buffered), seekable: ranges(media.seekable) };
}
function observeMedia(media) {
  const start = performance.now(), events = [], samples = [];
  let eventCount = 0, sampleCount = 0;
  const snapshot = () => ({ ms: Math.round(performance.now() - start), time: media.currentTime,
    network: media.networkState, ready: media.readyState, paused: media.paused,
    seeking: media.seeking, ended: media.ended });
  const retain = (list, item, limit) => {
    if (list.length === limit) list.splice(limit / 2, 1);
    list.push(item);
  };
  const listener = event => { eventCount++; retain(events, { event: event.type, ...snapshot() }, 40); };
  const names = ['play', 'playing', 'pause', 'waiting', 'stalled', 'seeking', 'seeked',
    'ended', 'error', 'canplay', 'canplaythrough', 'emptied', 'progress', 'suspend'];
  for (const name of names) media.addEventListener(name, listener);
  const timer = setInterval(() => { sampleCount++; retain(samples, snapshot(), 16); }, 250);
  return { stop() {
    clearInterval(timer);
    for (const name of names) media.removeEventListener(name, listener);
    return { events, samples, event_count: eventCount, sample_count: sampleCount,
      records_truncated: eventCount > events.length || sampleCount > samples.length };
  } };
}
async function advance(media, timeout = 12000, observation = {}, original = media.currentSrc) {
  const before = media.currentTime;
  const video = media.tagName === 'VIDEO';
  const qualityBefore = video ? media.getVideoPlaybackQuality().totalVideoFrames : null;
  const started = performance.now(), trace = observeMedia(media);
  observation.initial = mediaState(media, original);
  observation.play_promise = 'not-called';
  let first = null, last = null, callbacks = 0, callbackId = null, running = true;
  try {
    if (video) {
      check(typeof media.requestVideoFrameCallback === 'function', 'Native video frame callbacks unavailable');
      const frame = (_, metadata) => {
        if (!running) return;
        const sample = { media_time: metadata.mediaTime, presented_frames: metadata.presentedFrames };
        first ||= sample;
        last = sample;
        callbacks++;
        callbackId = media.requestVideoFrameCallback(frame);
      };
      callbackId = media.requestVideoFrameCallback(frame);
    }
    observation.play_promise = 'pending';
    const promise = media.play().then(() => {
      if (running) {
        observation.play_promise = 'fulfilled';
        observation.promise_settled_ms = Math.round(performance.now() - started);
        observation.at_promise_settlement = mediaState(media, original);
      }
    }, error => {
      if (running) {
        observation.play_promise = 'rejected';
        observation.promise_error = errorText(error);
      }
      throw error;
    });
    await within(promise, 'Media play', timeout);
    await waitFor(() => media.currentTime >= before + .3 && (!video ||
      (callbacks >= 2 && last.media_time > first.media_time && last.media_time >= before + .2)),
      'Playback time or actual video frames did not advance', timeout);
    media.pause();
    const paused = media.currentTime;
    await delay(250);
    check(media.paused && Math.abs(media.currentTime - paused) < .1, 'Pause did not hold position');
  } finally {
    running = false;
    if (callbackId !== null) media.cancelVideoFrameCallback(callbackId);
    Object.assign(observation, { before_time: before, after_time: media.currentTime, video_callbacks: callbacks,
      first_frame: first, last_frame: last, quality_before: qualityBefore,
      quality_after: video ? media.getVideoPlaybackQuality().totalVideoFrames : null,
      final: mediaState(media, original), elapsed_ms: Math.round(performance.now() - started),
      trace: trace.stop() });
  }
  return observation;
}
function isReleased(state) {
  return state.network_state === HTMLMediaElement.NETWORK_EMPTY &&
    state.ready_state === HTMLMediaElement.HAVE_NOTHING && state.paused && !state.seeking &&
    !state.has_src && !state.has_src_object && state.source_children === 0 &&
    state.buffered.count === 0 && state.seekable.count === 0;
}
async function released(media, timeout = 12000, original = '', observation = {}, stability = 300) {
  let callbackId = null, callbacks = 0;
  observation.initial = mediaState(media, original);
  try {
    await waitFor(() => isReleased(mediaState(media, original)), 'Cancel retained playable media state', timeout);
    const before = media.currentTime;
    observation.stable_before_time = before;
    if (media.tagName === 'VIDEO') {
      check(typeof media.requestVideoFrameCallback === 'function', 'Native video frame callbacks unavailable');
      callbackId = media.requestVideoFrameCallback(() => { callbacks++; });
    }
    await delay(stability);
    check(isReleased(mediaState(media, original)) && Math.abs(media.currentTime - before) < .001 && callbacks === 0,
      'Canceled media continued to advance');
  } finally {
    if (callbackId !== null) media.cancelVideoFrameCallback(callbackId);
    Object.assign(observation, { final: mediaState(media, original), new_video_frames: callbacks,
      stability_ms: stability });
  }
  return observation;
}
async function playback(fixture) {
  const {kind, url, duration, route, name} = fixture;
  const media = document.createElement(kind);
  media.id = 'plain-native-' + name.replaceAll('.', '-');
  media.muted = true;
  media.volume = 0;
  media.preload = 'metadata';
  if (kind === 'video') { media.width = 320; media.height = 240; }
  document.body.append(media);
  const result = { kind, fixture: name, route, verification_method: 'real-webview-media-events', status: 'failed', stages: [], playback_evidence: [] };
  diagnostic('media-started', { fixture: name, route });
  const play = async phase => {
    const observation = {};
    try { await advance(media, 12000, observation, url); }
    finally {
      const { trace, ...summary } = observation;
      result.playback_evidence.push({ phase, ...summary });
      check(await diagnostic('playback-observation', { fixture: name, phase, ...observation }),
        'Playback diagnostic acknowledgement failed');
    }
  };
  try {
    await event(media, 'loadedmetadata', () => { media.src = url; media.load(); });
    check(Math.abs(media.duration - duration) < .25, 'Unexpected media duration');
    mediaStage(result, 'metadata');
    await play('initial');
    mediaStage(result, 'play-pause');
    for (const target of [duration * .92, duration * .1]) {
      const seeking = observeMedia(media), before = mediaState(media, url);
      try { await event(media, 'seeked', () => { media.currentTime = target; }); }
      finally {
        check(await diagnostic('seek-observation', { fixture: name, target, before,
          after: mediaState(media, url), trace: seeking.stop() }), 'Seek diagnostic acknowledgement failed');
      }
      check(Math.abs(media.currentTime - target) < .3, 'Wrong seek position');
      await play('seek:' + target);
      mediaStage(result, 'seek-play-pause:' + target);
    }
    media.pause();
    await event(media, 'emptied', () => { media.removeAttribute('src'); media.load(); });
    result.cancel_after_emptied = mediaState(media, url);
    result.cancel_observation = {};
    await released(media, 12000, url, result.cancel_observation);
    mediaStage(result, 'cancel');
    await event(media, 'loadedmetadata', () => { media.src = url; media.load(); });
    await play('reload');
    mediaStage(result, 'reload-play');
    result.status = 'passed';
  } catch (error) { result.error = errorText(error); result.failure_state = mediaState(media, url); }
  finally { media.pause(); media.removeAttribute('src'); media.load(); media.remove(); }
  check(await transmit('result', result), 'Result acknowledgement failed');
  diagnostic('media-completed', { fixture: name, status: result.status });
}
async function imageLoad(fixture, retain) {
  const image = document.createElement('img');
  document.body.append(image);
  const result = { kind: 'image', fixture: 'image.png', route: 'production-native-scheme',
    verification_method: 'real-webview-image-load-and-decode', status: 'failed' };
  diagnostic('image-started', { retain });
  try {
    await event(image, 'load', () => { image.src = fixture.url; });
    await image.decode();
    check(image.naturalWidth === fixture.width && image.naturalHeight === fixture.height,
      'Unexpected decoded image dimensions');
    result.width = image.naturalWidth;
    result.height = image.naturalHeight;
    result.retained = retain;
    result.status = 'passed';
  } catch (error) { result.error = errorText(error); }
  finally { if (!retain) { image.removeAttribute('src'); image.remove(); } }
  check(await transmit('result', result), 'Result acknowledgement failed');
  diagnostic('image-completed', { status: result.status, retain });
}
async function deniedImage(url, stage) {
  const image = document.createElement('img');
  document.body.append(image);
  const result = { kind: stage, fixture: 'image.png', route: 'production-native-scheme',
    verification_method: 'real-webview-image-error-and-rust-status', status: 'failed' };
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(Error('Denied resource did not report an error')), 12000);
      image.onerror = () => { clearTimeout(timer); resolve(); };
      image.onload = () => { clearTimeout(timer); reject(Error('Foreign image decoded')); };
      image.src = url;
    });
    result.status = 'passed';
  } catch (error) { result.error = errorText(error); }
  finally { image.removeAttribute('src'); image.remove(); }
  check(await transmit('result', result), 'Result acknowledgement failed');
}
function failed(error) { diagnostic('script-failed', { error: errorText(error) }); }
window.afterOwnerWarm = () => (async () => {
  await deniedImage(urls.foreign, 'foreign-owner');
  await emit('foreign-denied');
})().catch(failed);
window.afterOwnerClose = () => (async () => {
  await deniedImage(urls.foreign, 'closed-owner');
  await imageLoad(urls.image, false);
  await emit('foreign-complete');
})().catch(failed);
window.__plainNativeTestSupport = { transmit, advance, released, waitFor };
if (urls.unitOnly) return;
window.addEventListener('error', event => diagnostic('script-error', { error: errorText(event.error || event.message) }));
window.addEventListener('unhandledrejection', event => diagnostic('unhandled-rejection', { error: errorText(event.reason) }));
diagnostic('script-started');
(async () => {
  check(window.origin === 'tauri://localhost', 'Test document must have the nonopaque Tauri App origin');
  check(location.href === 'tauri://localhost/native-media-test.html', 'Unexpected test document URL');
  if (urls.foreign) {
    await emit('foreign-ready');
  } else {
    for (const fixture of urls.media) await playback(fixture);
    await imageLoad(urls.image, true);
    await emit('owner-ready');
  }
})().catch(failed);
})();
