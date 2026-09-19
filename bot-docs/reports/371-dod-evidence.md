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
| Android host tests/build | 25 SMS contracts passed; assembleDebug passed in 3m21s |
| Emulator acceptance | Incoming/open-thread, missed-event (60.2s), new conversation, offline recovery, outgoing sent/readback and failed-send draft restoration passed |
| Physical-phone acceptance | Debug installed alongside release; setup awaits unlock |
| #369 implementation | Task-removal and wake-lock repairs verified; Android full suite 1,075 pass / 1 skip; explicit Stop and background SMS verified on emulator |
| Push / PR | Ready for scoped review; do not auto-close unconfirmed permanent-missing-SMS report |

The real-device sample is metadata-only and read-only. No carrier SMS was sent;
a debug APK was installed alongside the unchanged release. The original browser
tab was not reloaded or navigated.
The production frontend attempt ran in separate headless Chromium, not the user's
visible browser. No unkillable-service or complete-sync-repair claim is justified.
