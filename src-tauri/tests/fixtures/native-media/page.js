(() => {
if (window.__plainNativeMediaTest) return;
window.__plainNativeMediaTest = true;
const reports = [];
const urls = FIXTURE_URLS;
function errorText(error) {
  if (error instanceof DOMException) return error.name;
  return String(error).replace(/https?:\/\/[^\s"'<>]+/g, '[HTTP resource]');
}
function check(value, message) { if (!value) throw Error(message); }
function delay(ms) { return new Promise(resolve => setTimeout(resolve, ms)); }
function event(media, name, action) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => finish(Error('Timeout: ' + name)), 12000);
    const ok = () => finish();
    const bad = () => finish(Error('Media error code ' + media.error?.code));
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
  try {
    await event(media, 'loadedmetadata', () => { media.src = url; media.load(); });
    check(Math.abs(media.duration - duration) < .25, 'Unexpected media duration');
    result.stages.push('metadata');
    await advance(media);
    result.stages.push('play-pause');
    for (const target of [duration * .92, duration * .1]) {
      await event(media, 'seeked', () => { media.currentTime = target; });
      check(Math.abs(media.currentTime - target) < .3, 'Wrong seek position');
      await advance(media);
      result.stages.push('seek-play-pause:' + target);
    }
    media.pause();
    await event(media, 'emptied', () => { media.removeAttribute('src'); media.load(); });
    check(media.networkState === HTMLMediaElement.NETWORK_EMPTY, 'Cancel retained a media resource');
    result.stages.push('cancel');
    await event(media, 'loadedmetadata', () => { media.src = url; media.load(); });
    await advance(media);
    result.stages.push('reload-play');
    result.status = 'passed';
  } catch (error) { result.error = errorText(error); }
  finally { media.pause(); media.removeAttribute('src'); media.load(); media.remove(); }
  reports.push(result);
}
async function denied(url, stage) {
  const media = document.createElement('video');
  media.muted = true;
  media.volume = 0;
  document.body.append(media);
  const result = { kind: stage, route: 'production-native-scheme', verification_method: 'real-webview-media-error-and-rust-status', status: 'failed' };
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(Error('Denied resource did not report an error')), 12000);
      media.onerror = () => { clearTimeout(timer); resolve(); };
      media.onloadedmetadata = () => { clearTimeout(timer); reject(Error('Foreign resource metadata leaked')); };
      media.src = url; media.load();
    });
    result.status = 'passed';
  } catch (error) { result.error = errorText(error); }
  finally { media.removeAttribute('src'); media.load(); media.remove(); }
  reports.push(result);
}
window.afterOwnerClose = async () => {
  await denied(urls.foreign, 'closed-owner');
  await playback(urls.control);
  document.title = 'plain-result:' + JSON.stringify(reports);
};
(async () => {
  if (urls.foreign) {
    await denied(urls.foreign, 'foreign-owner');
    document.title = 'plain-foreign-ready:' + JSON.stringify(reports);
  } else {
    for (const fixture of urls.media) await playback(fixture);
    document.title = 'plain-result:' + JSON.stringify(reports);
  }
})().catch(error => {
  document.title = 'plain-harness-error:' + errorText(error);
});
})();
