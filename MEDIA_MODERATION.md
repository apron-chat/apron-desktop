# Explicit media and scoped moderation

This milestone extends the native Rust/GPUI client without adding a browser,
codec player, background fetch policy or authentication dependency.

## User flows

- Upload embeds expose **Open image viewer…**. Opening does not fetch. **Load
  image** uses the existing single-transfer actor. The viewer offers Fit, 100%,
  bounded 10–400% zoom, scroll to pan, Close/Escape and Save PNG.
- Click author initials or choose **Room and members → Members → Avatar…** to
  inspect an avatar. It loads only after an explicit action; **Show in chat**
  stores its thumbnail for this session. **Hide loaded avatars** clears that cache.
  Unknown, changed, failed or blocked avatars fall back to initials.
- **Set my avatar…** chooses and normalizes one local raster, previews it, then
  requires **Confirm upload**. This uses `/avatar` with one upload embed, not a
  chat message. The uploaded image is shared profile data; the dialog says so.
- **Remove with reason…** collects an exact user ID and a one-line reason, then
  shows a separate room/user/reason review before **Confirm removal**. It invokes
  the reference server's `/kick` command with one explicit mention. It removes
  membership, not a ban. The server enforces creator/moderator policy. Denial
  preserves the form; uncertain completion never triggers an automatic retry.

The command capability does not enumerate individual commands. `/avatar` and
`/kick` are implemented by the inspected reference server and may be rejected by
another server; the UI labels moderation as server-specific. No generic ban,
admin, role escalation or guessed moderation RPC was added.

## Bounds and privacy

HTTP media must match the connected server's scheme/host/port; there are no
redirects or forwarded credentials. Secure servers require HTTPS. HTTP is allowed
only for an explicitly selected loopback test server. No arbitrary external-origin
avatar is fetched, even when advertised. Inline avatars accept only bounded
base64 PNG/JPEG/WebP/GIF sources (64 KiB URI limit). SVG/documents are rejected.

Raster decoding retains the existing 8 MiB input/output, 4096-pixel-side,
eight-million-pixel and 64 MiB decoder limits. Animation uses its first frame.
Metadata is removed. One viewer holds a full-size raster at a time; Close releases
its application-held image/bytes. Avatar cache: at most 32 thumbnails, each at
most 128×128, keyed by user and source. A changed source invalidates its display
without fetching the new URL. GPU resource reclamation remains toolkit-managed.

Loading, uploads and attachment transfers share one actor transfer lane. Closing
a loading viewer cancels it; late results cannot populate a closed/stale viewer.
A decode already executing may finish before cancellation takes effect. Avatar
upload cancellation can be uncertain if the server has committed the file; users
must inspect their profile before retrying. No auto-retry or persisted image cache.

Save PNG writes normalized bytes through the existing atomic private-file helper,
refuses overwrite and does not re-fetch the original. The actual Save portal UI
still needs desktop validation; its file-write/no-overwrite helper is tested.

## Validation

All data is fictional and all servers/compositors are owned isolated instances.
Core and native results and exact binary hashes are in `docs/media-moderation`.
The reference Go server remains `0670838`; protocol `3df659a` and web `e09522d`.

- 52 core/fixture tests; nine opt-in isolated server tests.
- Official-server creator removal, denied ordinary-member removal, private-room
  send denial after removal, shared reason notice, avatar upload and raster fetch.
  The subsequent MULTICLIENT_VALIDATION.md milestone also tests an actual
  locally granted moderator-role account through the official development sender.
- Native daily suite: 22 check groups at minimum and wide sizes, including
  light/dark captures. It exercises viewer fit/zoom/Escape, cancellation and data
  release, avatar upload confirmation, no auto-load/cache before consent, explicit
  display, and moderation review with no request followed by server denial.
- Screenshot critique changed the viewer width/image budget and replaced the
  clipped moderation review with a compact exact room/user/reason summary. A native
  cancellation check also found that programmatic toolkit dialog close bypasses
  close callbacks; Close/Cancel now explicitly clean up before dismissing.
- Exact-head CI is recorded in PR #2. Local Nix is unavailable; the CI Nix job
  builds x86_64 UI, checks core tests and installs the package. Native/loopback
  suites run separately here. Actual niri/hardware and accessibility/IME remain
  unverified; headless Weston is not a substitute for those desktop checks.

## File transfer interoperability, precisely

The previous milestone implemented composer `ExternalPaths` drops, a pointer
modality adapter after keyboard input, bounded regular-file staging, cancellation,
directory rejection and explicit Send. The native test dispatches GPUI platform
FileDropEvent Entered/Submit events: the application event path passes. **A real
file manager's compositor drag data offer has not been tested.**

Clipboard-file support uses `wl-paste` only for `text/uri-list` and GNOME copied-file
MIME offers, with a 64 KiB/three-second read limit and at most four local file URIs.
Plain text is not interpreted as paths. **Actual URI-list clipboard offers were
tested through wl-copy/wl-paste and the owned Wayland compositor.** The GNOME MIME
branch is implemented but has no actual file-manager interoperability run. Native
PNG/JPEG image clipboard and ordinary text paste were also tested. File chooser
and Save portal interaction, real niri, and arbitrary file-manager variants remain
external desktop validation work.

## Remaining

Audio/video playback is deferred pending dependency/runtime-weight choices.
Cross-origin avatars are intentionally unsupported. Passkeys remain unimplemented;
[PASSKEY_DESIGN.md](PASSKEY_DESIGN.md) describes the exact origin/RP trust boundary,
cooperating browser handoff and native CTAP alternatives and their blockers.
No authenticator, real auth provider, production chat, bot or credentials were used.

## Reviewed native previews

[Wide viewer](docs/media-moderation/wide/report-viewer-fit.png) ·
[Dark avatar preview](docs/media-moderation/minimum/report-avatar-dark.png) ·
[Minimum permission error](docs/media-moderation/minimum/report-moderation-denied.png).
