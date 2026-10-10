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
| Message edit/delete | Own-message contextual UI, stale target checks, server permission errors; no uncertain retries |
| Reactions | Deferred UI |
| Selection and copy | Rendered-text selection and whole-message contextual copy |
| Markdown and links | Native headings/lists/code/emphasis/quotes, compose preview; sanitized HTML/images, inert links |
| Unread and mentions | Bounded live/fetched-history reconciliation, optional saved cursors, ID-backed autocomplete and recipient chips |
| Read receipts | Advance-only, best-effort supported activity; focused tail auto-read |
| Typing, presence, notifications | Deferred UI |
| Attachments | Explicit bounded upload/download; no redirects, same origin, no overwrite |
| Images | Native clipboard raster paste, removable preview, explicit same-server embedded-image load; bounded normalization/cache |
| Drag/drop and clipboard files | Deferred |
| Connection recovery | Heartbeat, bounded channels, backoff and explicit status/error states |
| Keyboard | Send, room filter/toggle, history, mark read, menus, reply and copy |
| Appearance | Toolkit semantic light/dark themes and relative sizing; settings UI deferred |
| Persistence | Opt-in private bounded draft/reply/recipient/read files; reset controls; tokens/history/attachments never saved |
| NixOS/niri | Reproducible dev-shell inputs and launch instructions; actual niri/hardware unverified |
| Accessibility/IME | Widget semantics and keyboard tests; assistive software/IME unverified |

Unread counts describe received/fetched evidence, not every unfetched offline
message. Reconnect/room entry reconciles retained history against saved read IDs.
Offline edits that add mentions to older already-read messages are not recovered
as new notifications. Targets evicted by history bounds require manual pagination.
See README for limits, clipboard upstream constraints and profile concurrency.
