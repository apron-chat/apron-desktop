# Verification — 2026-10-10, apron-gtk readiness follow-up

All network testing used synthetic data on dedicated loopback listeners. No
production service, real credential, bot process/state, or real chat content
was used. Existing reference repositories remained unchanged. The project was renamed from apron-native to apron-gtk while preserving Git
history, legacy local-state lookup, and the original keyring schema. Source is
on its own `feat/native-wayland` branch. Publication to the user-designated apron-chat/apron-gtk feature branch is
separately authorized; main and deployment remain outside this milestone.

## Actual checks

- **46 unit/fixture tests passed** (`test_*.py`): 36 state/action/rendering/
  preferences/keyring/local-state/recovery/presence/naming tests, 6 libsoup WebSocket
  integration tests against the fictional fixture at `127.0.0.1:18765`, and
  4 media tests including a synthetic loopback HTTP server.
- **4 official Go reference integration tests passed**, separately from the
  fixture tests. Built unchanged checkout
  `067083815be6b113e7d35f6569fa35dee0052fd5` with Go 1.27.1. Server identifies
  as `apron-go/8`; it ran at `127.0.0.1:18766/ws` with `--store memory`, a
  dedicated synthetic upload directory, and `--messages-per-minute 10000`.
- **GTK workflow passed at 1280×900 and 640×900**, headless Weston/Wayland:
  room/thread navigation, older pages, per-room drafts, denied-send draft
  retention, confirmed replies and screenshots.
- **Native daily-action dialogs passed against the official Go server**:
  repeated Create/Save activation while pending, editing through a TextView,
  stale-snapshot refusal, reconnect during an open editor and refusal to save
  in the changed connection, profile update, loaded-cache search, room details,
  preferences cancel, lazy native message menu, and actual clipboard readback.
- **Design regression flow passed at both 1280×900 and 640×900** on real GTK:
  light/dark surfaces, keyboard-focused message actions, repeated lazy menus,
  incoming snapshot while a contextual menu is open, older/latest shortcuts,
  profile/preferences forms, empty/loading states, disabled disconnected actions,
  retained unsent draft, and capability refresh after reconnect.
- **Native lifecycle/recovery flow passed**: unchanged row identity and scroll
  anchor preservation, cancelling bottom-follow when scrolling during an incremental
  fill, rapid room switches during load, 50-row/cache bounds, interrupted reconnect,
  fresh-window restoration of a synthetic draft/reply/mentions, actual Preferences
  reset confirmation, disk deletion, and no rewrite on close.
- **Archived-page official-server reconciliation passed** with the cache cleared:
  later edits, deletes and moves replace original snapshots before display.
- **Official-server presence/attachment test passed**: typing to another
  connection, idle/attended transitions, timed room mute/unmute, upload embed
  creation, HTTP PUT, final snapshot, byte-identical GET and rejected reuse of
  the single-use write URL. The reference server rejects numeric idle with
  `invalid_params`; the tested compatibility fallback uses boolean idle for
  that connection, as the inspected web client does.
- **Native presence/media flow passed**: new app ID, real composer user-action
  signal, typing row, mute dialog, muted notification suppression, native upload
  confirmation with two-file reply, repeated-submit guard, byte-identical explicit
  save, composer clearing after acceptance and preservation of newer edits,
  partial failure stopping a three-file batch, and stale attachment refusal. File chooser callbacks were supplied synthetic
  local paths deterministically; native dialogs, Gio file IO, protocol post and
  HTTP transfers were real. Interactive file chooser/portal selection itself
  was not automated.
- HTTP safety tests verify absent Authorization/Cookie/Referer, blocked redirects
  and cross-origin URLs, bounded unknown-length bodies, cancellation and local
  file reads. All data and file paths were synthetic.
- Final follow-up reran all 46 unit/fixture tests, all four Go integration tests,
  and the native presence/media, daily-action and persistence/recovery flows.
  The new attachment confirmation screenshot was visually inspected
  (`docs/attachments-confirm.png`). Prior desktop/compact design captures remain applicable; design/performance
  suites were not rerun for this bounded follow-up.
- **Nix 2.26.3 validation passed**: parse, x86_64 package derivation evaluation,
  then `nix flake check --no-build --all-systems` for both supported architectures
  (packages, apps and dev shells). Generated `flake.lock`. Nix and its store
  lived under `/tmp`; no daemon, system install or security setting changed.
  No Nix package build or niri launch was performed. See `docs/nix-check.txt` and `docs/readiness-ui.txt`.
- Python syntax compilation, `git diff --check`, and wheel build passed.
  Actual GTK screenshots were visually inspected at desktop and compact sizes,
  including dark theme, focus disclosure, forms and empty state. Captures are
  included under `docs/design/`; prior previews are in `docs/design-before/`.
  Screenshot capture retries bounded frame readiness after focus transitions;
  it does not edit or synthesize the rendered images.

## Coverage details

Pure tests cover numeric identifiers, replay/edit/move/delete and same-log
redaction, embedded replies, explicit mentions versus text, unread/read cursors,
retention, inclusive log pagination, byte/count limits, safe URLs/Markdown,
complete/partial identity state, membership/reaction replay, preservation of
message client fields, stale-action checks, preference allowlisting and 0600
atomic files, unavailable-keyring fallback, async Secret Service adapter calls,
and serialized token rotation followed by Forget.

Fixture tests cover guest auth, token rotation, rejected tokens with no guest
fallback, listings, history pages, send/reply/mention encoding, thread creation,
denied sends, uncertain delivery and reconnect without automatic resend.

Official-server tests cover guest connect/list/history/send/reply/thread/
reconnect; 65-post backward pagination; profile name; private-room creation;
room title/description changes; edit preserving mentions/replies; repeated
reaction sets and clearing; private-thread inheritance; move/delete; leaving a
private thread; and leaving/rejoining a public room. Private-thread rejoin after
leaving can fail as unknown because the former member can no longer see it;
this was server policy, and the UI now warns that an invitation may be needed.
The server may give a guest a fresh identity after reconnect. These tests do
**not** establish official-server token-resume or account-auth interoperability.

The keyring tests use a mock Secret Service API, not a running credential vault.
The local environment lacks its GI bindings/service. Notifications use native
Gio APIs with opt-in UI, but actual desktop-daemon delivery is unverified.

## Reproduction

See README for installing dependencies and starting the fixture. Then:

```sh
python -m unittest discover -s tests -p 'test_*.py' -v
GDK_BACKEND=wayland python tests/ui_smoke.py /tmp/apron-preview.png
GDK_BACKEND=wayland python tests/design_smoke.py /tmp/apron-design
GDK_BACKEND=wayland python tests/ui_recovery_smoke.py

# With the isolated official Go server above running:
python tests/reference_interop.py
GDK_BACKEND=wayland python tests/ui_daily_smoke.py /tmp/apron-daily.png
GDK_BACKEND=wayland python tests/ui_presence_media.py /tmp/apron-presence.png
python scripts/seed_reference.py > /tmp/apron-benchmark-seed.json
APRON_BENCH_ROOM=$(python -c 'import json; print(json.load(open("/tmp/apron-benchmark-seed.json"))["room_id"])') \
  GDK_BACKEND=wayland python tests/benchmark_ui.py
```

In this workspace, each Python invocation used
`scripts/local-env.sh /usr/bin/python3`, isolated native libraries under
`.deps`, `GSETTINGS_BACKEND=memory`, `XDG_CACHE_HOME=/tmp/apron-cache`, and
`APRON_NATIVE_STATE_DIR=/tmp/apron-preferences-test` for GTK flows. Weston used
headless/pixman, the kiosk shell, no configuration file, no idle timeout, and
separate 1280×900 and 640×900 outputs. Client rendering used `GSK_RENDERER=cairo`.
`dbus-run-session` supplied the synthetic test session bus.

Versions: Python 3.13, GTK 4.18.6, libadwaita 1.7.6, libsoup 3.6.5,
Weston 14.0.2, markdown-it-py 3.0.0, websockets 15.0.1.

## Prior milestone responsiveness measurement

Performance was not remeasured after the presence/attachment additions.
The same 1,000-message, 1,045-byte-text synthetic benchmark room was used for
both the visual-design baseline and incremental rendering. Both use native GTK,
headless Weston/pixman and Cairo. No production traffic or content was used.

| Measurement | After design, before incremental | Current incremental + reconciliation |
|---|---:|---:|
| Latest benchmark page, complete | 518.6 ms | 881.8 ms |
| Ten older pages, min / median / max | 482.5 / 501.6 / 543.8 ms | 825.2 / 926.5 / 1320.6 ms |
| Worst 10-ms heartbeat interval | 501.5 ms | 151.1 ms |
| Idle wall / process CPU | 10.241 s / 0.0061 s | 9.885 s / 0.0071 s |
| Idle percentage of one core | 0.060% | 0.072% |
| Client RSS | 77.7 MiB | 78.5 MiB |
| Rendered messages | 50 | 50 |
| Cached messages / serialized bytes | 549 / 671,427 | 583 / 678,227 |

Worst observed load-time gap fell about **70%** in the final run. An earlier
incremental run observed 67.1 ms; the final run above is deliberately the reported
result. Full-page completion is slower: the client yields between batches and
older pages now include forward reconciliation traffic. This reduces long
blocking turns but does not establish a 16-ms frame budget. True viewport
virtualization remains useful, especially for large or complex individual rows.

The benchmark now waits for all rendering batches, and initializes its heartbeat
clock when the timer is installed, avoiding counting prior startup as a load
stall. An intermediate run exposed that measurement bug after startup contained
more synthetic general-room messages. The original design baseline's maximum
was a page-load stall, greater than its entire 117-ms startup. General-room test
data grew between runs, so startup and total cached message counts are not direct
comparisons; the benchmark room stayed unchanged. Local-state storage was off
for these performance samples. These are single warm shared-workspace runs,
not isolated repeated statistical samples, actual niri hardware, or a soak test.

Raw evidence: `docs/benchmark-design.json`, `docs/benchmark-incremental.json`.
The prior daily-chat baseline below is retained for comparison across the visual
pass; its 548-ms stall was measured before the design changes.

## New state/recovery coverage

Tests cover endpoint/user isolation, fresh disk restore, file mode 0600,
1-MiB/8-account/256-room bounds, oversized/corrupt input, allowlisting, coalesced
writes across accounts, reset ordering, and draft restoration before an async
write finishes. A blocked-delete regression also verifies that restore cannot
resurrect disk drafts while a reset is active. Reset tests use a temporary directory and fictional text only.
Normal-close flush and file/directory fsync are implemented; abrupt power loss
and simultaneous independent application instances were not tested.

Forward-scan tests cover inclusive continuation, a fixed horizon as the server
advances, edits/deletes/moves, retention invalidating a selected page, missing
progress, scan budget exhaustion, cancellation, and newer live snapshots winning.
Real libsoup tests recover missed unread/mentions from already cached snapshots,
verify idempotent counts, and discard cancelled room results. Recovery is limited
to opened rooms and 128 sequential requests; incomplete recovery is reported.

## Previous daily-chat efficiency measurement

These measurements used the expanded daily-chat UI **before the design pass**
and the official Go server. Performance has not been remeasured after the
visual changes. A separate
public room contained 1,000 synthetic messages of 1,045 text bytes each. The
client opened its latest 50-record page and ten older pages. Record limits
include membership changes, so a page need not contain exactly 50 messages.

| Measurement | Observed |
|---|---:|
| Startup to initial room history | 163.1 ms |
| Benchmark-room latest page | 503.3 ms |
| 10 older pages: minimum / median / maximum | 506.2 / 525.7 / 598.9 ms |
| Worst 10-ms heartbeat interval during loading | 548.1 ms |
| Idle wall / process CPU | 10.05 s / 0.0011 s |
| Idle CPU, percentage of one core | 0.011% |
| Client RSS / peak RSS | 78.7 / 78.7 MiB |
| Cached messages / serialized bytes | 591 / 679,827 |
| Rendered messages | 50 |

Timings include history handling, widget updates and draining GTK's main
context; explicit 50/100-ms settling pauses are subtracted. A 10-ms GLib timer
measured responsiveness and was removed before idle measurement. Idle uses
GLib's blocking main loop, process CPU time and `/proc/self/status` RSS, excluding
server/compositor resources. The initial benchmark incorrectly expected 50
messages rather than 50 log records; that harness assertion was corrected.

Raw pre-design results: `docs/benchmark.json`. Before avoiding the redundant
nonempty-page rebuild at loading start, pages took 988–1,362 ms, with a 738.7-ms
worst heartbeat interval (`docs/benchmark-before.json`). Message menus are now
created lazily. These changes reduced page-load work, but **long main-thread
stalls remain**. The next performance step is row virtualization/incremental
layout. This single warm local sample is not a load/soak test or a comparison
against Rust, apron-web, GPU rendering, or actual niri hardware.

## Not run / remaining validation

- Nix package build and actual niri launch. Evaluation passed; the workspace has
  no normal `/nix/store` installation, and niri is unavailable. Full build/runtime
  validation belongs on a NixOS host or dedicated Nix builder.
- Full protocol conformance, comprehensive arbitrary-history recovery, aliases,
  real TLS endpoints, actual Secret Service operation, actual OS notifications,
  interactive file chooser/portal selection, inline media playback,
  passkeys/email, closed-app push, long-duration load and assistive-tech audit.
- Performance targets across hardware and large/high-churn multi-room workloads.

See FEATURE_PARITY.md for the implemented/partial/missing/intentionally-different
matrix. This remains an expanded prototype, not complete apron-web parity.
