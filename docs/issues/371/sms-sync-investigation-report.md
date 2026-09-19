# SMS synchronization investigation — #371

Status: investigation and implementation in progress (2026-09-19).

## Reproduced: mixed pending/provider ordering

The thread sorted provider rows and then appended pending SMS and MMS. Thus a
pending message timestamped 13:58 appeared below a provider message at 13:59,
matching the ordering pattern in the owner's screenshot. This is a frontend
projection defect, independent of Android process suspension.

A failing-first hook regression reproduced the reversed order in real headless
Chromium. Sorting the combined list corrects it without changing provider data.
The focused message-thread, message-helpers and sms-state-sync suites pass: 26 tests.
This proves the code-level ordering correction, not yet its installed-device behavior.

## Other symptoms under investigation

Missing incoming messages, disappearing sends, terminal send status and Android
background service reliability require separate evidence. Current upstream already
preserves successful pending SMS as Sent when provider readback does not find them.
The test phone already has READ_SMS and a battery-optimization exemption; simply
asking for those permissions again would not explain the observed behavior.

No claim is made yet that all reported symptoms have the same cause or are fixed.

## Reproduced: refresh recovery gaps

Failing-first tests demonstrated that repeated provider events could postpone the
debounced refresh indefinitely, returning to a visible tab did not trigger readback,
and slow reads could overlap. The shared thread/sidebar scheduler now caps event
batching at two seconds, coalesces triggers during a read into one follow-up, and
refreshes on visibility/focus/network restoration. A visible, online Messages view
also reads back once per minute if events are missed entirely. It removes timers and
listeners on deactivation. This is a recovery bound while the browser is scheduled
and the backend is reachable, not a guarantee during OS suspension.

Background refresh retains the loaded thread/conversation window size and avoids
forcing a reader of older messages to the bottom. Readers already at the bottom
continue following incoming messages. Safety reads add network/provider work while
Messages remains visible, and can keep the server's inactivity window active.

## Live evidence and limitations

A read-only comparison on the authorized Pixel 9 Pro (Android 17, PlainApp 3.3.25)
found all 16 SMS provider rows newer than 2026-09-18T00:00:00Z in a fresh authenticated
API response: eight incoming and eight outgoing, with matching IDs. No private
message contents or identifiers are included here. This sample does not reproduce
permanently missing incoming rows. The issue author has been asked for transport
type (SMS/MMS versus RCS), phone-side visibility and browser-reload behavior.

Testing the new production frontend against the installed 3.3.25 backend exposed
upstream schema incompatibilities: App.features, Query.deviceStatus and
Query.audioQueueItems are absent in that release. The SMS query itself returned
99 rows for a sampled recent thread, but the new frontend could not render a usable
thread in that mixed-version setup. The phone's installed release and its settings
have not been replaced. A matching debug APK was installed alongside the release;
physical-phone setup awaits unlocking the device.

A matching 3.4.0 debug backend was subsequently built and installed in a disposable
Android 12 emulator. The production frontend, served through a loopback-only test
proxy, logged in using the real pairing flow. Emulator-only SMS injection verified:

- An incoming SMS appears in an already-open thread without reloading.
- Suppressing server-to-browser WebSocket frames causes a missed event; the actual
  message is recovered by the safety read after 60.2 seconds, without reloading.
- A new sender/conversation is discovered without WebSocket updates or reloading.
- Going offline and back online recovers a message received during the interruption.
- An emulator-only outgoing SMS resolves to a sent provider row and remains visible.
  With the emulator radio disabled, the failed send restores the draft instead of
  leaving a Sending bubble. Neither test is a real carrier-delivery test.

The emulator tests do not prove carrier sending, RCS visibility, MMS attachments,
or every OEM's background restrictions. A test-harness cleanup call was unsupported
after the three assertions passed; it did not invalidate those observations.

## Verification at the first local milestone

- `corepack yarn test`: 1,142 passed, 51 skipped across 127 files. Live integration
  tests are among the skipped tests; these are not device verification.
- `corepack yarn typecheck`: passed.
- `corepack yarn build`: passed.
- `git diff --check`: passed.
- Android `:shared:testAndroidHostTest --tests '*SmsSyncContractTest'`: 25 passed.
- Android `:app:assembleDebug`: passed in 3m21s with a 10-minute command bound.
- Matching x86_64 Github debug build and real emulator/browser checks above passed.
- Timeouts are now estimated per command; 120 seconds is a default, not a hard cap.

Assignment was requested from the owner after GitHub denied self-assignment.
This is a scoped fix, not a final resolution claim for every #371 symptom. The
author's report of permanently absent incoming messages remains unconfirmed.
The related #369 Android change reproduces and repairs task-removal shutdown and
wake-lock reacquisition defects; its report is in the Android repository.
