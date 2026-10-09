# Verification — 2026-10-09, native design pass

All network testing used synthetic data on dedicated loopback listeners. No
production service, real credential, bot process/state, or real chat content
was used. Existing reference repositories remained unchanged. Native source is
on its own `feat/native-wayland` branch. No push, merge, or deployment occurred.

## Actual checks

- **25 unit/fixture tests passed** (`test_*.py`): 21 pure state/action/rendering/
  preferences/keyring-adapter tests and 4 actual libsoup WebSocket integration
  tests against the fictional Python fixture at `127.0.0.1:18765`.
- **2 official Go reference integration tests passed**, separately from the
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

# With the isolated official Go server above running:
python tests/reference_interop.py
GDK_BACKEND=wayland python tests/ui_daily_smoke.py /tmp/apron-daily.png
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

## Actual efficiency measurement

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

- Nix flake evaluation/build and actual niri launch (tools unavailable).
- Full protocol conformance, comprehensive arbitrary-history recovery, aliases,
  real TLS endpoints, actual Secret Service operation, actual OS notifications,
  passkeys/email, closed-app push, long-duration load and assistive-tech audit.
- Performance targets across hardware and large/high-churn multi-room workloads.

See FEATURE_PARITY.md for the implemented/partial/missing/intentionally-different
matrix. This remains an expanded prototype, not complete apron-web parity.
