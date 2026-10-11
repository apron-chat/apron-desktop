# Remaining native integrations

Reviewed against protocol repository `3df659a`, web reference `e09522d`, and
Go server `0670838`. The table below preserves the prior assessment. The raster viewer, explicit avatar
display/upload and scoped /kick flow are now implemented; see MEDIA_MODERATION.md
for their exact bounds and test status. Audio/video and passkeys remain deferred.

| Area | Existing foundation | Concrete next step | External validation or decision |
|---|---|---|---|
| Larger image viewer | Explicit same-origin fetching, bounded raster decode, thumbnails, Save | Implemented: explicit full-size raster viewer, bounded fit/zoom/scroll, Close/Escape, cancellation, stale-source handling and Save PNG. Timeline thumbnails remain separate. | Hardware memory/performance check; implementation itself is feasible locally. |
| Audio/video | Safe file transfer and explicit external links | Choose native playback dependencies and decoder/process limits, or an explicit “open in player” flow. Do not add a browser or autoplay solely for parity. | Native playback adds codecs and runtime weight; the product choice matters before adding that dependency stack. |
| Avatar display | User updates, deterministic initials, bounded raster pipeline | Implemented: explicit preview and Show in chat for same-server/inline rasters; 32 small thumbnails keyed by user/source, stale-source invalidation and initials fallback. Foreign origins remain blocked. | External-origin loading policy needs an explicit user-facing choice. Same-origin/inline-only implementation is feasible locally. |
| Avatar upload | Existing upload protocol and staging | Implemented: local raster preparation, shared-profile preview/confirmation, capability-gated `/avatar` upload and user update handling. | Synthetic server configuration is sufficient for implementation tests; no real account is required. |
| Moderation | Exact-ID membership forms and own-message edits/deletes | Implemented: exact-ID `/kick` with shared reason and separate confirmation. Creator and moderator success, ordinary-member denial, and membership permissions tested locally. No generic ban/admin RPCs; broader command UI remains a follow-up. | Server policy determines authorization. Broader commands are server-specific, not a generic native-platform blocker. |
| Passkeys | Separate-connection email auth and optional token/keyring sessions | Implement the two-step `auth` ceremony on one connection, discoverable credentials with user verification, bounded base64url WebAuthn JSON, cancellation/expiry and origin-bound resume tokens. An authenticator adapter is still missing. | Choose a native roaming-authenticator (CTAP2) path or a cooperating origin-hosted browser handoff. The latter needs server/frontend support; the former needs an authenticator and a defined native origin/RP trust policy. Neither is achieved by sending a browser's token or silently claiming its origin. |

Passkey integration has a specific trust boundary: the current Go server checks the
WebSocket origin against configured RP origins before starting a ceremony, and binds
both the pending ceremony and resume token to that origin. The current native
transport does not establish that native trust policy. Library support alone cannot
resolve it; registration/login must also be tested with an actual authenticator.
See protocol sections 4.8.6 and 4.10, and the current server's `webauthn.go` and
`commands.go`.

## Current platform interoperability limits

- Optional `dnd`/`invisible` are offered only when advertised. Idle is opt-in,
  reports seconds in the current protocol, and clears with `false`. The current
  Go server still explicitly requires the older boolean type; only that exact
  rejection permits a fallback. Unrelated errors or connection loss never cause
  that retry. Both forms have synthetic coverage.
- GPUI 0.3.8 exposes external file drops but not URI-list clipboard offers.
  Explicit paste therefore uses a bounded, three-second `wl-paste` adapter for
  file MIME types. Plain text is never interpreted as a filename. Nix supplies
  this helper; images continue through the existing GPUI clipboard path.
- The toolkit's file-drop translation retains keyboard modality after typing.
  The composer restores pointer modality on an external file drag so hover/drop
  checks work. This is an application event adapter, not a fork of widget code.
- Native platform-dispatched drop tests cover the application's event path.
  A real file manager's compositor data-offer interoperability, niri, hardware
  rendering, Secret Service, notifications, IME and assistive software remain
  separate desktop validation tasks.

No unknown dependency or user decision blocks ordinary preferences, attachment,
emoji, or scoped room-workflow development. Unimplemented work should remain
labelled as backlog rather than being described as environment-blocked.

Passkey options and precise blockers are now detailed in [PASSKEY_DESIGN.md](PASSKEY_DESIGN.md).
