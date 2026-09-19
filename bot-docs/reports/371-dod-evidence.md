# #371 / #369 verification ledger

Status: partial local milestone, not completion. See docs/issues/371/sms-sync-investigation-report.md.

| Gate | Evidence |
| --- | --- |
| Upstream baseline | desktop c49083c5; app 7d8fbf36, fetched and isolated |
| Ordering regression | Failed before combined sorting; passed after |
| Refresh recovery | Starvation, resume, serialization, cleanup and safety-read regressions |
| History window | Thread and sidebar requested-window tests failed before fixes, pass after |
| Full frontend tests | 1,142 pass; 51 skipped, including unconfigured live integration |
| Typecheck/build | Both pass |
| Live API/provider sample | 16 recent SMS IDs match, 8 incoming / 8 outgoing |
| New frontend + old APK | Fails compatibility: new schema fields absent in 3.3.25 |
| Android host tests/build | Pending: compile exceeds standing 120-second command limit |
| Installed-device acceptance | Pending matching Android build |
| #369 implementation | Pending; inspected service stop-on-task-removal and wake-lock paths |
| Push / PR | Not yet; end-to-end gate outstanding |

The real-device sample is metadata-only and read-only. No SMS was sent, no APK
removed or installed, and the original browser tab was not reloaded or navigated.
The production frontend attempt ran in separate headless Chromium, not the user's
visible browser. No unkillable-service or complete-sync-repair claim is justified.
