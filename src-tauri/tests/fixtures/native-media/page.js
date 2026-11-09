(() => {
if (window.__plainNativeMediaTest) return;
window.__plainNativeMediaTest = true;
const reports = [];
const urls = FIXTURE_URLS;
let sequence = 0;
function emit(stage) {
  document.title = 'plain-event:' + JSON.stringify({
    stage, sequence: ++sequence, origin: window.origin, href: location.href, reports
  });
}
function errorText(error) {
  if (error instanceof DOMException) return error.name;
  return String(error).replace(/https?:\/\/[^\s"'<>]+/g, '[HTTP resource]');
}
function diagnostic(stage, detail = {}) {
  document.title = 'plain-diagnostic:' + JSON.stringify({
    stage, detail, reports, origin: window.origin,
    document_url: location.href === 'tauri://localhost/native-media-test.html' ? location.href : '[Unexpected document URL omitted]'
  });
}
window.addEventListener('error', event => diagnostic('script-error', { error: errorText(event.error || event.message) }));
window.addEventListener('unhandledrejection', event => diagnostic('unhandled-rejection', { error: errorText(event.reason) }));
diagnostic('script-started');
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
async function advance(media) {
  const before = media.currentTime;
  const frames = media.tagName === 'VIDEO' ? media.getVideoPlaybackQuality().totalVideoFrames : 0;
  await media.play();
  const deadline = performance.now() + 12000;
  while (media.currentTime < before + .3 && performance.now() < deadline) await delay(50);
  check(media.currentTime >= before + .3, 'Playback time did not advance');
  if (media.tagName === 'VIDEO') {
    check(media.getVideoPlaybackQuality().totalVideoFrames > frames, 'No decoded video frames');
  }
  media.pause();
  const paused = media.currentTime;
  await delay(250);
  check(media.paused && Math.abs(media.currentTime - paused) < .1, 'Pause did not hold position');
}
async function playback(fixture) {
  const {kind, url, duration, route, name} = fixture;
  const media = document.createElement(kind);
  media.muted = true;
  media.volume = 0;
  media.preload = 'metadata';
  if (kind === 'video') { media.width = 320; media.height = 240; }
  document.body.append(media);
  const result = { kind, fixture: name, route, verification_method: 'real-webview-media-events', status: 'failed', stages: [] };
  diagnostic('media-started', { fixture: name, route });
  try {
    await event(media, 'loadedmetadata', () => { media.src = url; media.load(); });
    check(Math.abs(media.duration - duration) < .25, 'Unexpected media duration');
    mediaStage(result, 'metadata');
    await advance(media);
    mediaStage(result, 'play-pause');
    for (const target of [duration * .92, duration * .1]) {
      await event(media, 'seeked', () => { media.currentTime = target; });
      check(Math.abs(media.currentTime - target) < .3, 'Wrong seek position');
      await advance(media);
      mediaStage(result, 'seek-play-pause:' + target);
    }
    media.pause();
    await event(media, 'emptied', () => { media.removeAttribute('src'); media.load(); });
    check(media.networkState === HTMLMediaElement.NETWORK_EMPTY, 'Cancel retained a media resource');
    mediaStage(result, 'cancel');
    await event(media, 'loadedmetadata', () => { media.src = url; media.load(); });
    await advance(media);
    mediaStage(result, 'reload-play');
    result.status = 'passed';
  } catch (error) { result.error = errorText(error); }
  finally { media.pause(); media.removeAttribute('src'); media.load(); media.remove(); }
  reports.push(result);
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
  reports.push(result);
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
  reports.push(result);
}
function failed(error) { diagnostic('script-failed', { error: errorText(error) }); }
window.afterOwnerWarm = () => (async () => {
  await deniedImage(urls.foreign, 'foreign-owner');
  emit('foreign-denied');
})().catch(failed);
window.afterOwnerClose = () => (async () => {
  await deniedImage(urls.foreign, 'closed-owner');
  await imageLoad(urls.image, false);
  emit('foreign-complete');
})().catch(failed);
(async () => {
  check(window.origin === 'tauri://localhost', 'Test document must have the nonopaque Tauri App origin');
  check(location.href === 'tauri://localhost/native-media-test.html', 'Unexpected test document URL');
  if (urls.foreign) {
    emit('foreign-ready');
  } else {
    for (const fixture of urls.media) await playback(fixture);
    await imageLoad(urls.image, true);
    emit('owner-ready');
  }
})().catch(failed);
})();
