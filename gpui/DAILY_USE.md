# Daily-use feature pass — 2026-10-10

The GPUI client now has opt-in draft/read persistence, contextual edit/delete,
member-backed mention completion, Markdown composition/preview, and native
clipboard image staging with explicit embedded-image loading. These changes remain
on `experiment/gpui-comparison`; the GTK worktree and main were not modified.

## Implemented and checked

| Area | Implementation | Evidence |
| --- | --- | --- |
| Local state | Off by default; per-server/user profile; ordered private atomic writes, bounded validation, disable/erase/reset | Unit tests cover isolation, permissions, corruption, symlinks and save/reset ordering; native save/reset checked |
| Read state | Advance-only cursors; reconcile fetched history after reconnect/room entry; bounded deduplicated mentions | Core reconciliation tests; native background unread, history reading and mark-read checks |
| Edit/delete | Own-message contextual actions gated by `edit`; preserve body/reply/embed IDs; reject stale targets; explicit delete confirmation | Official server enforces author permissions; native edits, deletion, stale rejection and repeated Escape dialogs |
| Markdown | Native headings, emphasis, lists, quotes and code; sanitized HTML/images; inert links; compose preview | Sanitization tests and native screenshots |
| Mentions | Actual supplied room identities, ID-based recipient chips, arrows/Enter completion, incoming @ You label and unread navigation | Identity/query tests; native selection produces explicit recipients; membership deltas reject older per-user records |
| Images | Actual PNG/JPEG clipboard offers; removable worker-normalized previews; never auto-send; protocol upload; explicit same-server thumbnail load | Native external `wl-copy` checks, normal text paste, unsupported-upload UI; real loopback uploads and decoded downloads |
| Boundaries | 8 MiB file total, four files, 4096 px per side, 8 million pixels, 64 MiB decoder budget, eight 640×480 thumbnail cache entries | Oversize/dimension/active-format tests; URL-origin/redirect/overwrite tests |

`cargo test --locked --no-default-features` passes 25 tests (19 unit,
3 authentication, 3 media). Four additional opt-in loopback tests pass against the
isolated official Apron server and the synthetic interruption fixture. Release
build and warnings-as-errors Clippy checks passed. Tests used no production
credentials or messages.

Native feature runs completed at **1280×900, 800×900 and 640×480**, under Weston 14
headless/pixman with Vulkan llvmpipe. They check Markdown, actual text/PNG/JPEG
clipboard formats, explicit mention recipients, two uploads, no automatic image
load, bounded decoded display, local-state reset, edit/delete, stale targets and
repeated dismissal. Capability absence was injected locally to verify the
unsupported-upload UI without modifying the server. Pixel/byte rejection is
covered at the decoder/transfer layer, not by a giant native clipboard offer.

The interaction regression also checks whole-message copy, native selection,
external clipboard text, **0 px prepend-anchor change**, selection preservation,
stale response rejection, rapid room switching, replies, draft restore,
permission errors and interruption without automatic resend. Menu/auth/filter,
unread and light/dark/large-text screenshots are retained separately.

## Visual decisions

- The composer has explicit Attach, Mention and Preview actions and a strong Send
  action. Its secondary shortcut hint disappears at narrow widths.
- The attachment tray and recipient chips have bounded scrolling regions. Image
  previews cannot expand the composer without limit, including at 640×480.
- Image loading is an explicit control next to each server upload. Save controls
  fit their content rather than stretching across the transcript.
- Incoming mentions have a text label as well as subtle shading. Recipient chips
  make the notification side effect reviewable before sending.
- Stale edits show one inline explanation and disable Save. Delete uses a danger
  action. The Local data dialog states the storage location, unencrypted nature,
  bounds and reset consequences.

The focus regression deliberately scrolls a row header partly outside the viewport,
then tabs into its actions and asserts that the header is fully visible. The fix
runs after frame completion so virtual-list layout cannot overwrite its offset.
Mouse focus is excluded. Before/after native captures are included below.

## Evidence index

- [Desktop image display](docs/daily/desktop/image-loaded.png),
  [compact image preview](docs/daily/compact/image-preview.png),
  [minimum-size preview](docs/daily/minimum/image-preview.png)
- [Mentions](docs/daily/compact/mentions.png),
  [unsupported upload](docs/daily/compact/unsupported-upload.png),
  [local data](docs/daily/compact/privacy.png),
  [stale edit](docs/daily/minimum/stale-edit.png)
- [Clipped before focus](docs/daily/desktop/partial-before-focus.png),
  [corrected keyboard focus](docs/daily/desktop/keyboard-message-focus.png)
- [Core tests](docs/daily/core-tests.txt), [official loopback](docs/daily/loopback.txt),
  [native desktop](docs/daily/desktop.json), [compact](docs/daily/compact.json),
  [minimum](docs/daily/minimum.json), [selection/reconnect regression](docs/daily/smoke.json),
  [benchmark](docs/daily/benchmark.json), [Nix evaluation](docs/daily/nix.txt)

## Performance scope

A warm synthetic 1,000-message run retained 549 transcript records and measured
147.2 MiB RSS. The first 10-second post-load interval used 18.7% of one CPU core
with 27 row renders; the following settled interval measured **0 CPU ticks and
0 row renders**. Latest-page state update took 86.6 ms; older pages ranged
5.7–231.6 ms and the maximum 10 ms UI heartbeat interval was 125.4 ms.
This run shared CPU resources with build work: latency is diagnostic evidence,
not a controlled before/after comparison or GPU presentation measurement. Raw
results are included. No hardware-GPU or niri performance claim is made.

## Remaining limits

- GPUI buffers clipboard offers before the app receives them; the 8 MiB limit is
  an application/decode limit, **not a bound on upstream clipboard ingestion**.
- Counts cover received/fetched evidence. Unfetched offline rooms and offline
  edits adding mentions to already-read messages are not fully recovered.
- Profile files are private but unencrypted. Multiple instances writing the same
  profile are not coordinated. Abrupt termination can lose a pending debounce.
  Tokens remain in memory, so restart requires signing in as the same user.
- Upload creates a message before transferring bytes. Cancellation/failure may
  leave a posted attachment placeholder; there is no automatic resend or retry.
  Selected attachments clear on room/session changes and are not persisted.
- Member lists may be server-truncated; completion currently excludes whitespace
  or display-control IDs and IDs over 128 bytes. Display names never guess IDs.
- Reactions, room creation/moderation, drag/drop, notifications, keyring login,
  account/email/OAuth flows and full web-client parity remain deferred.
- Actual niri, hardware GPUs, IME/assistive software, portals, clean Nix builds
  and aarch64 compilation were not run. Both pinned Nix dev-shell architectures
  evaluated successfully, including the native clipboard test dependency.
