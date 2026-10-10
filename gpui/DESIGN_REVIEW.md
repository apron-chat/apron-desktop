# GPUI design cleanup

The primary task is reading and responding to a conversation. The previous view
repeated Reply and Copy controls on every message and put reconnect beside normal
history navigation. Those commands consumed attention without helping reading.

## Changes driven by the review

- Replaced repeated labels with one contextual ellipsis, revealed on hover or
  keyboard focus and held visible while its menu is open. Right-click opens the
  same native menu. Reply, whole-message copy and text selection remain available.
- Kept sending visible. Moved session access, mark-read, latest and shortcut help
  into the conversation menu. Reconnect appears only when unavailable; rejected
  authentication has a direct **Sign in…** recovery action.
- Added room filtering and parent-adjacent thread ordering. Unread counts and
  explicit `@` mention counts share the trailing navigation lane; they do not
  depend on color alone. Mention shading preserves the common text alignment.
- Added a masked, session-only token form with clear draft-discard consequences,
  explicit guest access, inline validation and standard dismissal.
- Retained one transcript inset and stable author/time/action lanes. The message
  scroller still owns virtualization, prepend anchoring and its conditional latest
  button. The composer remains stable at the bottom of the window.

## What native inspection found

The first menu-copy run lacked a compositor input serial, so Wayland refused
clipboard ownership. The isolated test seat supplied one genuine key serial; the
same Reply/Copy workflow then passed. This was a harness issue, not a relaxed test.

Enter in the first token form reached the generic dialog close action before the
input event callback could submit. A scoped `Session > Input` submit action fixed
that ordering. Enter now submits and Escape releases the form. App input shortcuts
are scoped under `Apron` so they do not override modal input behavior.

The first auth-error screenshot offered Retry even though the token had been
rejected, and the sidebar incorrectly called an unpopulated list “No matching
rooms.” The final view offers Sign in and explains that rooms appear after
connecting. These are fixes prompted by actual rendered states.

At larger text in the 800-pixel window, the composer’s hint initially pushed Send
partly offscreen. Allowing the hint to shrink/wrap and preserving the buttons’
width fixed it. Narrow content panes move Load earlier into the menu. A lingering
tooltip covered the menu’s first row; tooltips now disappear while their menu is
open. The final captures verify both corrections.

## Verified evidence

- `cargo test --locked --no-default-features`: 16 tests passed (ten unit, three
  auth integration, three media integration); three opt-in loopback tests excluded
  from that command and run separately.
- The three loopback tests passed against a dedicated in-memory official Apron server
  and synthetic fixture: pagination, replies, exact-byte upload/download,
  single-use upload URLs, reconnect, interrupted-send handling and joining without
  editing room metadata. The final protocol audit corrected an inherited
  `room_set` join request to `room_join`; live rename/leave deltas now update the
  room index, with a unit regression for membership preservation and stale renames.
- Optimized Linux build and strict all-target Clippy passed.
- Native polish checks exercise real GPUI window pointer/key dispatch: contextual
  reply target, copy, keyboard menu navigation, room-filter focus, background
  mentions, preserving unread while reading earlier history, keyboard mark-read,
  token masking, Escape dismissal, rejected-token error and valid-token session.
  The 800-pixel run also traverses Tab to a visible message-actions focus ring and
  replies using R, and opens the actual toolbar session menu with keyboard input.
- Screenshots include normal, menus, reply, empty filter, unread, sign-in, masked
  token, auth error, empty room, long Markdown, dark theme and larger text.
  All content and tokens are synthetic. These are actual Wayland captures.

Raw reports and selected screenshots are in `docs/`. The opt-in `--polish-smoke`
mode only accepts the dedicated loopback fixture. It writes a `.stage` file for
external screenshots, and its short checkpoint timers never run in ordinary use.
Use `scripts/capture-checkpoints.py REPORT.json -- weston-screenshooter` alongside
the native check with a fresh report path to collect checkpoint images.
The virtual test seat is compiled from `scripts/weston-test-seat.c` against Weston
14 development headers. Set `APRON_TEST_SEAT_INPUT=1` on the isolated compositor and
launch the app within ten seconds to provide clipboard ownership. See
COMPARISON.md for the environment and distinctions from physical input testing.

## Remaining critique and limits

This remains a deliberately small desktop client. Room discovery is capped, search
is local, timestamps repeat, and long author names truncate. A later pass should
add a real message-search flow, richer room management and tested navigation to
unloaded reply/unread targets before adding decorative UI.

The review covers default and larger text plus light/dark themes in the headless
compositor. It is not a claim about all locales, fractional scaling, physical
keyboard/IME, assistive software, file chooser portals, hardware GPU performance or
actual niri behavior. Selection across unrendered rows also remains unverified. A partially visible top
row can receive a partly clipped focus ring; full keyboard-driven scroll-into-view
for that row remains a follow-up. Its menu and reply action are operable. At the 640×480 minimum, low-frequency
history controls move into the menu and the composer omits its hint when space is
tight, retaining the complete Send button.
Tokens are not persisted, but ordinary UI/network memory is not fully hardened
against process-memory inspection. Unread state is session-local and bounded.

## Preview and result index

- [Desktop](docs/preview.png) · [compact](docs/preview-narrow.png) ·
  [minimum size](docs/polish/minimum/wide.png)
- [Message menu](docs/polish/desktop/message-menu.png) ·
  [toolbar menu](docs/polish/compact/conversation-menu.png) ·
  [keyboard focus](docs/polish/compact/keyboard-message-focus.png)
- [Sign-in at minimum size](docs/polish/minimum/sign-in.png) ·
  [masked token](docs/polish/compact/token-masked.png) ·
  [auth recovery](docs/polish/minimum/sign-in-error.png)
- [Unread](docs/polish/compact/unread.png) ·
  [long Markdown](docs/polish/desktop/long-message.png) ·
  [dark theme](docs/polish/desktop/dark.png) ·
  [larger text at minimum size](docs/polish/minimum/large-text.png)
- Native reports: [desktop](docs/polish-desktop.json),
  [compact](docs/polish-compact.json), [minimum](docs/polish-minimum.json),
  [selection/reconnect regression](docs/polish-smoke.json)
- [Unit/integration output](docs/polish-tests.log),
  [official/fixture loopback output](docs/polish-interop.log),
  [Clippy](docs/polish-clippy.log), [build](docs/polish-build.log),
  [Nix evaluation](docs/polish-nix-check.txt)

The 1280×900 capture predates the final adaptive narrow-toolbar refinements;
800×900 and 640×480 captures exercise those. The final minimum-size report and
selection/reconnect regression ran against the final application build. The
original screenshots remain in `docs/design-before*.png` for comparison.
