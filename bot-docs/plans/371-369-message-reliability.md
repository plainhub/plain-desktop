# Message and background reliability

## Scope and baseline (2026-09-19)

Fix plain-app #371 first, then #369. Desktop baseline: c49083c5;
Android baseline: 7d8fbf36. Work in isolated sibling clones; preserve the owner's
existing checkouts. Never force push. Reports belong in docs/issues/<number>/.
The phone and logged-in browser are authorized diagnostic targets; avoid logging
message bodies or authentication. Carrier tests may use only the owner's previously
authorized test number. Do not retry carrier sends automatically.

## Contracts and territory

- Android telephony provider is authoritative for readable SMS/MMS. RCS and
  OS-restricted provider rows must not be misrepresented as an event-sync defect.
- Send acceptance, sent callback, provider readback and delivery are distinct.
  Keep successful optimistic messages until confirmed readback; never label carrier
  acceptance as recipient delivery. Retain upstream terminal-result tracking.
- Provider queries and pending SMS/MMS feed message-thread's combined chronological
  projection, consumed by MessageChatList, forwarding and exports.
- Provider/notification/socket events feed both message-thread and messages-sidebar.
  Recovery must not starve under event churn or require a page reload. Any background
  readback must respect view lifecycle, avoid overlapping requests and preserve scroll.
- HTTP service startup, task removal, explicit stop, wake-lock policy and network
  recovery form the Android service lifecycle. Respect explicit user stop and OS
  restrictions; do not promise an unkillable service.

## Phases and gates

1. Reproduce chronological ordering in the real hook's existing Chromium test harness.
   Fix the final projection, preserve source arrays and thread filtering. Run:
   `timeout 120 corepack yarn test --project=unit tests/hooks/message-thread.test.ts tests/lib/message-helpers.test.ts tests/lib/sms-state-sync.test.ts`.
2. Trace missing-event and pending-message paths. Add failing-first refresh/reconciliation
   tests for each confirmed defect, including concurrency and lifecycle cleanup.
   Inspect live browser/phone metadata before changing the installed APK. Separate
   observed device failures from synthetic regression evidence.
3. For #369, reproduce service/wake-lock lifecycle defects with Android host tests.
   Implement narrowly; verify explicit Stop remains terminal and background recovery
   does not leak locks, spin, or spawn duplicate server instances.
4. Run desktop typecheck, complete tests and production build, defaulting to 120s.
   Choose timeouts based on expected work, not a fixed cap: start Android host tests
   and `./gradlew :app:assembleDebug` with a 10-minute bound and monitor progress.
   The owner's clarified rule prevents indefinite hangs, not legitimate long builds.
   Exercise actual browser
   and device after the fixes: sends/readback, missed update recovery, foreground to
   background, network loss/reconnect, and explicit Stop.
5. Review exact diff and upstream drift, commit coherent verified milestones, push
   fork branches, open linked PRs and attach sanitized reports. Request assignment.

## Evidence ledger

- Ordering test failed on baseline: provider 13:59 rendered before pending SMS 13:58
  and MMS 13:57. Sorting the combined projection passes all 26 focused tests.
- Phone release 3.3.25, Android 17, READ_SMS granted and battery exemption already
  enabled. Service was foreground specialUse during initial inspection.
- Not yet proven: live cause of missing incoming rows, MMS disappearance, all
  background disconnects. These remain investigation gates, not completion claims.
- Matching Android 3.4.0 debug built; real emulator/browser verification passes
  incoming/open-thread updates, missed-event recovery (60.2s), new conversations,
  network restoration and SMS sent/readback. Radio-off failure restores the draft.
- Android #369 changes live in isolated branch `fix-369-background-stability`.
  Task-removal shutdown reproduced before and repaired after; explicit Stop still
  releases the service/lock. Six wake-lock regressions and the manifest contract
  pass; full Android host suite is 1,075 passed / one skipped.
- Physical-phone release remains untouched; alongside debug setup awaits unlock.
  Publish scoped improvements with issue references, not claims that every symptom
  or all Android background restrictions are resolved.

## Completion

Record exact passing commands, failures, live-test results and remaining limitations
in issue reports and dod-evidence.md. Mocked callbacks alone are not device proof.
Each coherent phase is a revert unit; never publish with unresolved failing gates.
