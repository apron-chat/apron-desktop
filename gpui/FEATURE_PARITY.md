# GPUI product scope

This is a working Apron 8 client, not complete apron-web parity. Protocol reference
is the repository’s pinned `references/PROTOCOL.md`; the GTK baseline and reference
web client remain separate. The GPUI UI owns its state and delegates bounded I/O to
a Tokio actor. No web view or browser is embedded.

| Capability | GPUI status |
| --- | --- |
| Native Wayland rendering | Built and exercised under Weston 14 / CPU Vulkan |
| Guest, token and rotated-token resume | Implemented; RAM only; explicit rejection recovery |
| Email/OAuth, registration, durable keyring login | Deferred |
| Rooms and thread navigation | Implemented; local filter, parent grouping, join, rename/leave deltas; 256-room cap |
| Room creation, moderation, membership management | Deferred UI |
| Latest/older history | Pages of 50, bounded cache, log reconciliation, stale-response rejection |
| Sending and replies | Implemented; local room drafts; uncertain sends never auto-resend |
| Message edit/delete, reactions | Incoming message revisions/deletes reconcile; outgoing UI deferred |
| Selection and copy | Rendered-text selection and whole-message contextual copy |
| Markdown and links | Sanitized HTML/images; links inert; no automatic remote fetching |
| Unread and mentions | Session events only; bounded counters; server-provided mention IDs |
| Read receipts | Advance-only, best-effort supported activity; focused tail auto-read |
| Typing, presence, notifications | Deferred UI |
| Attachments | Explicit bounded upload/download; no redirects, same origin, no overwrite |
| Thumbnails, drag/drop, clipboard file paste | Deferred |
| Connection recovery | Heartbeat, bounded channels, backoff and explicit status/error states |
| Keyboard | Send, room filter/toggle, history, mark read, menus, reply and copy |
| Appearance | Toolkit semantic light/dark themes and relative sizing; settings UI deferred |
| Persistence | No token, history or draft persistence |
| NixOS/niri | Reproducible dev-shell inputs and launch instructions; actual niri/hardware unverified |
| Accessibility/IME | Widget semantics and keyboard tests; assistive software/IME unverified |

Unread is deliberately labelled session-local. Fetching older history does not
invent unread counts, and reconnect does not infer missed notifications. A count is
not a promise about every unread message on the server. Explicit history pagination
and cache limits also mean not every unread target remains navigable locally.
