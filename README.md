# apron-gtk

A working native Linux client for **Apron protocol 8**, built with Python,
GTK4, libadwaita, and libsoup 3. Native Wayland widgets; no Electron, WebView,
HTTP polling, or browser runtime. This is an early vertical slice, not a
complete replacement for apron-web. See [FEATURE_PARITY.md](FEATURE_PARITY.md)
for the source-reviewed functional matrix and remaining work.

![Native Wayland preview](docs/preview.png)

The application, command and package are named **apron-gtk**. Python modules use
`apron_gtk`; the desktop/Wayland application ID is `chat.apron.Gtk`.

Existing installations retain their saved data: if the new configuration
directory does not exist, apron-gtk reuses `~/.config/apron-native` (under
`$XDG_CONFIG_HOME` when set). New installs use `~/.config/apron-gtk`.
`APRON_GTK_STATE_DIR` overrides both; the old `APRON_NATIVE_STATE_DIR` remains a
fallback. The existing Secret Service schema is deliberately retained so saved
tokens remain discoverable. No credentials are copied to a file. Historical
benchmark logs and before-design screenshots retain the former name.

## Run on NixOS / niri

The flake pins nixpkgs to an exact revision and supplies the Python interpreter,
GTK4, libadwaita, libsoup, introspection data, desktop launcher, and icon.

```sh
# From an extracted source archive (path: also avoids Git's untracked-file filtering)
nix run path:.                         # connection dialog; no automatic remote connection
nix develop path:.
python scripts/demo_server.py          # terminal 1; fictional loopback-only server
GDK_BACKEND=wayland python -m apron_gtk.app --demo  # terminal 2, in the dev shell

# Build/install
nix build path:.
nix profile install path:.
```

After installation, launch **apron-gtk** from your application launcher. For niri,
an optional binding is `Mod+A { spawn "apron-gtk"; }` in the existing `binds`
block. It is an ordinary toplevel Wayland window with app ID `chat.apron.Gtk`;
it needs no XWayland or compositor-specific plugin. `GDK_BACKEND=wayland` can
force Wayland if troubleshooting backend selection. GTK follows the system's
appearance and scaling settings.

**Nix status:** the exact-revision flake is supplied but was not evaluated or
built here because this environment has no Nix executable. The first Nix run
can generate `flake.lock`; its nixpkgs input is already pinned. niri itself was
not available for testing. Actual native UI verification used Weston/Wayland.

## Run without Nix

Requires Python 3.11+, GTK 4.12+, libadwaita 1.4+, libsoup 3, PyGObject,
markdown-it-py. The demo additionally needs `websockets` (tested at 15.0.1).
On Debian 13:

```sh
sudo apt install python3-gi gir1.2-gtk-4.0 gir1.2-adw-1 gir1.2-soup-3.0 \
  python3-markdown-it python3-websockets
/usr/bin/python3 scripts/demo_server.py
# In another terminal, from this directory:
GDK_BACKEND=wayland /usr/bin/python3 -m apron_gtk.app --demo
```

Without `--demo`, enter your server's `wss://` address and either a guest
name or an existing bearer token. Guest/token must be offered by that server.
Remote plain `ws://`, URL credentials, and query parameters are rejected.
`ws://` is allowed only for `localhost`, `127.0.0.1`, and `::1` development.
Tokens stay in memory by default. **Preferences → Save session tokens in the
system keyring** is optional and requires libsecret plus an unlocked Secret
Service provider (for example a configured desktop keyring). Use **Use saved
session** in the connection dialog to retrieve one; Forget removes it. There
is no plaintext file fallback. The native keyring adapter is implemented, but
actual service operation was not testable in this environment. Message history is never written to disk. Drafts/read markers stay in memory
unless **Preferences → Save drafts and read markers on this device** is enabled. Connection/name and appearance
preferences can be remembered separately. Do not use a real token for tests.

In the supplied workspace, native dependencies were extracted under `.deps`
because system directories are read-only. This **workspace-only** launcher
uses them; `.deps` is intentionally omitted from source downloads:

```sh
scripts/local-env.sh /usr/bin/python3 scripts/demo_server.py
scripts/local-env.sh /usr/bin/python3 -m apron_gtk.app --demo
```

## Interface

The conversation has one clear header and composer. Account settings live in
the sidebar account menu; room details, members and thread creation live in the
header room menu. Reply and message actions appear on hover or keyboard focus;
right-click, Menu, or Shift+F10 also opens message actions. Existing reactions
remain visible. History controls appear only when relevant.

See [DESIGN.md](DESIGN.md) for the visual system, before/after captures, and
light, dark, compact, dialog and interrupted-connection previews.

## Local data and recovery

Local draft/read storage is **off by default**. When enabled, it stores plaintext
composers (including reply IDs and mention IDs) and read cursors in
`$XDG_CONFIG_HOME/apron-gtk/local-state.json` (normally `~/.config/apron-gtk/`).
Each profile is keyed by the exact validated server URL and authenticated user
ID. A new guest identity cannot inherit another identity's drafts. No tokens,
received messages, or room/member lists enter this file.

Storage is capped at 1 MiB total and 8 profiles, 256 rooms per profile, 32,000
UTF-8 bytes per draft and 768 KiB of drafts per profile. Older saved data is
evicted to stay within these limits. Writes coalesce after 750 ms of changes,
run on one worker, use atomic replacement with mode 0600 and fsync, and flush on
normal close. A crash can lose changes in the debounce interval. Concurrent
client instances use last-writer-wins files; cross-process merging is not supported.

Disabling storage deletes the saved file. **Reset local drafts and read markers…**
asks for confirmation, disables storage, clears current drafts/read markers and
queues deletion for every saved server/profile. It does not delete server
messages, preferences or keyring tokens. Use the separate keyring Forget action
for saved authentication. Filesystem deletion cannot erase backups or guarantee
secure physical erasure.

History remains bounded in RAM. An older page is replayed forward to its captured
log horizon before display, covering later edits, deletes, moves and reactions.
Only the selected page's projection is held during this scan. Opened rooms also
recover available unread/mentions from a known read cursor on latest/reconnect;
recovered history never emits desktop notifications. Each scan allows at most
128 sequential 50-record requests. Expired history or the scan budget produces
an explicit warning; an unverifiable older page is not committed. This is not
a durable full-room log cache or background recovery of every room.

## Typing, presence and attachments

Typing uses `activity`: 15-second indicators, refreshed at most once every
12 seconds during user edits, stopped after a five-second pause or room switch.
Programmatically restored drafts do not announce typing. Incoming indicators
expire locally and stop on a message; the map is capped at 256 participants.

With `status`, five minutes without input to this window reports idle; input
returns it to attended. A connection starting unattended reports that state.
Older servers rejecting numeric idle get a boolean fallback for that connection.
The account menu and room menu expose notification mutes: 15 minutes, one hour,
until unmuted, or unmute. Server notifications determine the stored mute state.
Global/parent-room mutes and Do Not Disturb suppress local mention notifications.
These settings remain server-owned; there is no new background polling loop.

With `embed:upload`, the composer attachment icon opens a native file chooser,
then a confirmation with an optional caption. Join the room first. One file up
to **8 MiB** is read, confirmed and posted as an upload embed; its returned
single-use `write_url` receives an HTTP PUT. The existing composer draft is kept.
The caption is a separate message; attaching to the current reply or sending
multiple files together is not implemented. Failed/uncertain transfers are never
automatically retried; check the message before posting another attachment.

Received uploads show pending metadata or **Save attachment…**. Saving requires
an explicit destination and downloads at most 8 MiB. Nothing is opened or
executed automatically. Transfers allow only the connected server's HTTP(S)
origin, HTTPS remotely and HTTP for the existing loopback development connection.
They use a separate HTTP session with no bearer token, cookies, referrer or
redirect following. Cross-origin/CDN URLs are visibly unsupported even when
legitimate; this conservative restriction is intentional. The transfer can be
cancelled and has a two-minute operation timeout. There is no automatic media fetch, inline decoder, audio/video player,
iframe/HTML renderer, stream viewer, attachment queue or upload resumption.
The server may reject a file type/size below the client limit; errors stay visible.

## What works

- Actual asynchronous WebSocket connection with TLS through libsoup, protocol
  greeting, guest or token authentication, in-memory token rotation, response
  correlation, 20-second request timeouts, advertised ping/pong, and randomized
  exponential reconnect backoff (approximately 1–31 seconds).
- Room directory, thread navigation/discovery, joining a listed room, creating
  rooms/private rooms/threads, title/description editing, confirmed leave, member
  listing and exact-ID add/remove, and live room/user/membership updates. No `rooms` capability falls back
  to the default room, learned from messages.
- 50-record history requests, older/latest navigation, numeric inclusive log
  cursors using `first_log_id - 1`, snapshot replacement, moves/tombstones,
  embedded replies, and retention pruning. No history capability means live
  scrollback only.
- Confirmed sends, replies, join-before-send for unjoined rooms, explicit
  member-picker mentions, per-room drafts,
  visible errors, and uncertain-delivery feedback. A disconnected send is
  **never automatically repeated**; check history before sending again.
- In-session unread counts and explicit mention badges; history does not create
  notifications. Read markers advance when the active window is at the bottom
  of the latest page. Own activity cursors from another connection are honored.
- Message **⋯** menus: native clipboard copy, edit/delete/move your messages,
  and start a thread from a message. Edits preserve embeds/mentions/replies/ext;
  stale local editors and changed connections are rejected rather than silently
  overwriting newer state. Pending dialogs guard repeated submissions.
- Reaction counts/toggles and a small native picker; history/live replay per user.
- Profile display-name and advertised status picker. Native light/dark/system
  appearance and opt-in content-free mention notifications through Gio. Real
  desktop notification delivery is still unverified in the headless environment.
- **Find** searches only the bounded loaded-message cache, not all server history.
- CommonMark formatted text through a restricted Pango renderer; raw HTML is
  escaped, images/embeds have text fallbacks; attachment downloads require an explicit Save action.
  Links are displayed as text and do not execute or open automatically.
- Adaptive sidebar below 760px; selectable message text; native focus behavior.
  `Ctrl+K` finds rooms, `Ctrl+F` searches loaded messages, `Ctrl+L` focuses the
  composer, `Ctrl+Enter` sends, `Alt+Up` opens the parent, `Alt+N` finds the next
  unread room, and `Escape` cancels a reply. `Ctrl+PageUp` loads older history and `Ctrl+PageDown` returns to latest.
  The account menu lists shortcuts. Enter in a room list activates it.

## Bounds and architecture

`model.py` is a GTK-independent state reducer. `session.py` owns libsoup and
request/timer lifecycles on the GLib main context. `recovery.py` performs bounded
fixed-horizon scans; `local_state.py` validates and asynchronously persists
optional draft/read state. `app.py` builds native GTK
widgets, reuses unchanged rows and creates at most two new rows per 16-ms batch; `ui_actions.py` owns dialogs, `actions.py` preserves
message fields, `preferences.py` persists allowlisted non-secret settings, and
`keyring.py` delegates credentials exclusively to Secret Service. `render.py` converts CommonMark to an allowlisted
Pango subset. No worker polling loop is used.

At most 50 messages are rendered; the shared snapshot cache is capped at
2,000 records **and 8 MiB of serialized message data**, with LRU eviction.
There are at most 256 room records, 2,048 current users, 256 drafts, 64 pending
requests, and 256 KiB per incoming WebSocket frame. Badges cap at 999. GTK and
Python overhead is additional to the serialized cache budget. UI text is
bounded and control/bidi override characters are removed for display.

## Deliberate limitations

- This is not full protocol conformance. Passkeys, email sign-in, durable
  chat/session-cache recovery, multiple simultaneous accounts, rich interactive embeds,
  bulk message operations, commands, calls, and server push are
  deferred. The keyring adapter and Gio notifications require real desktop-service
  validation; they are not yet verified end to end. The public Apron demo may permit
  guest reading but require passkeys for writing; use the local fixture or a
  server that permits guest/token writes for this slice.
- Reconnect discards the received-message cache, restores eligible local read
  cursors, and loads the selected room. Forward recovery is bounded to 128 pages
  per scan, with explicit incomplete/retention feedback. No durable log checkpoint,
  all-room recovery, or offline message cache is implemented.
- Room membership state replays, but timeline membership-event grouping,
  user-ID alias/rename reconciliation, and full live status semantics are incomplete.
  Unread/mention badges cover newly delivered creations, not mention additions
  to edits. Room listings over the 256-room cap are truncated.
- The Markdown renderer preserves common emphasis, headings, lists, code, and
  quotes, but is not a rich document viewer; links and media remain inert text.
- Valid TLS certificates are required; there is no “ignore certificate errors”
  switch. No SSH transport, proxy configuration UI, or background tray mode.

## Measured performance

The prior milestone’s 1,000-message loopback benchmark (1,045 text bytes per message) measured
**151 ms worst 10-ms heartbeat interval**, down from **502 ms** after the visual
design pass and **548 ms** in the earlier daily-chat build. Final client RSS was
**78.5 MiB**, with **0.072% of one CPU core** over a 9.89-second idle sample.

The tradeoff is longer full-page completion: latest 882 ms; ten reconciled older
pages 825–1,321 ms (median 927 ms). GTK yields between row batches and unchanged
rows retain identity and scroll position. There are still long frames; this is
incremental rendering, not viewport virtualization or a frame-time guarantee.
Measurements are single warm software-rendered Weston runs, not general targets.
Presence/attachment additions have not been rebenchmarked. See TEST_RESULTS
and `docs/benchmark-*.json` for method and limitations.

## Verification

Start the **local fixture only**, then run:

```sh
python -m unittest discover -s tests -p 'test_*.py' -v
# With a Wayland compositor running and the fixture still running:
GDK_BACKEND=wayland python tests/ui_smoke.py /tmp/apron-preview.png
GDK_BACKEND=wayland python tests/design_smoke.py /tmp/apron-design
GDK_BACKEND=wayland python tests/ui_recovery_smoke.py
```

The fixture binds only `127.0.0.1:18765`, uses fictional people/messages, keeps
state in memory, and is a test harness rather than a deployable server. Its
`/test-denied` and `/test-disconnect` message texts deliberately exercise errors.
Do not expose it to a network or use it with real data.

The official Go reference server was also built and tested separately on
loopback with synthetic data and `--store memory`; connect/list/history/send/
reply/thread/reconnect passed. Reproduce using an isolated official server:

```sh
# Build apron-server-go, then run its binary in a dedicated test directory:
aprond --store memory --addr 127.0.0.1:18766 --messages-per-minute 10000 \
  --upload.dir ./test-uploads
# From this client, in its Python/native development environment:
python tests/reference_interop.py
GDK_BACKEND=wayland python tests/ui_daily_smoke.py /tmp/apron-daily.png
GDK_BACKEND=wayland python tests/ui_presence_media.py /tmp/apron-presence.png
python scripts/seed_reference.py > /tmp/apron-benchmark-seed.json
APRON_BENCH_ROOM=$(python -c 'import json; print(json.load(open("/tmp/apron-benchmark-seed.json"))["room_id"])') \
  GDK_BACKEND=wayland python tests/benchmark_ui.py
```

These scripts hard-code the dedicated loopback test endpoint. `--server
ws://127.0.0.1:18766/ws` also opens it in the client as a guest. Never point
these tests at a server with real content.

See [TEST_RESULTS.md](TEST_RESULTS.md) for actual checks, versions, commands,
and unrun checks. Screenshots show actual GTK rendering under headless Weston,
not an HTML mockup. Source references and pinned commits are in
[references/README.md](references/README.md).

License: MIT; the included protocol reference retains its upstream license.
