# GPUI product scope

This is a working Apron 8 client, not complete apron-web parity. Protocol reference
is the repository’s pinned `references/PROTOCOL.md`; the reference web client and alternative GTK implementation remain external. The GPUI UI owns its state and delegates bounded I/O to
a Tokio actor. No web view or browser is embedded.

| Capability | GPUI status |
| --- | --- |
| Native Wayland rendering | Built and exercised under Weston 14 / CPU Vulkan |
| Guest, token and rotated-token resume | Implemented; RAM default, opt-in Secret Service storage; explicit rejection recovery |
| Email and keyring | Email proposal/code on a separate unauthenticated connection; synthetic test passed. Keyring load/store/clear/rotation implemented; real provider/service validation unrun |
| Passkeys / other auth | Native WebAuthn not implemented; no generic OAuth protocol assumed |
| Rooms and thread navigation | Implemented; fuzzy switcher, collapsible parent grouping, join, rename/leave deltas; 256-room cap |
| Room/thread management | Create, title/description settings, leave, member list and exact-ID add/remove forms; server policy enforced. Confirmed exact-ID /kick with reason; other moderation commands deferred |
| Latest/older history | Pages of 50, bounded cache, log reconciliation, stale-response rejection |
| Sending and replies | Implemented; local room drafts; uncertain sends never auto-resend |
| Message edit/delete | Own-message contextual UI, stale target checks, server permission errors; no uncertain retries |
| Reactions | Capability-gated four-choice submenu, aggregate/toggle chips, ordered replacement sets, removal and bounded history reconciliation; searchable Unicode/skin-variant picker; complete archive census deferred |
| Selection and copy | Rendered-text selection and whole-message contextual copy |
| Markdown and links | Native headings/lists/code/emphasis/quotes, compose preview; sanitized HTML/images, inert inline links; explicit safe HTTP(S) browser action |
| Unread and mentions | Bounded live/fetched-history reconciliation, optional saved cursors, ID-backed autocomplete and recipient chips |
| Read receipts | Advance-only, best-effort supported activity; focused tail auto-read |
| Typing, presence, mutes | Expiring typing, online/opt-out presence, member presence labels and nested inherited mutes; advertised DND/invisible and opt-in focus/five-minute idle; synthetic same-account/peer validation passed |
| Notifications | Durable opt-in, body-free native notifications, focus/mute/DND gating; actual daemon delivery unverified |
| Attachments | Explicit bounded upload/download; staged and sent title editing; no redirects, same origin, no overwrite |
| Images | Native raster paste, thumbnails and explicit same-server full viewer with Fit/zoom/scroll/Save PNG; bounded normalization/cache |
| Avatars | Explicit same-server/inline preview and session display, 32×128px thumbnails; confirmed normalized upload through /avatar; foreign origins blocked |
| Drag/drop and clipboard files | Bounded explicit staging; real URI-list clipboard tested, platform-dispatched drop tested; actual file-manager drop still unverified |
| Connection recovery | Heartbeat, bounded channels, backoff and explicit status/error states |
| Conversation search | Ctrl+F, result count, Enter/F3/Shift+F3, focused match; retained current-room text only |
| Unread room filter | All rooms / Unread, based on bounded received/fetched attention |
| Keyboard | Send, room filter/toggle, history, mark read, menus, reply and copy |
| Appearance | Refined opaque light/dark palette, durable profile theme, grouped authors and local initials |
| Persistence | Opt-in private bounded draft/reply/recipient/read files; reset controls; bounded attention metadata; tokens only in optional system keyring, history bodies/attachments never saved |
| NixOS/niri | Pinned package/app and dev shell, desktop entry/icon; baseline Nix CI build passed; exact-head CI in PR #2; actual niri/hardware unverified |
| Accessibility/IME | Widget semantics and keyboard tests; assistive software/IME unverified |

Unread counts describe received/fetched evidence, not every unfetched offline
message. Reconnect/room entry reconciles retained history against saved read IDs.
Reconnect replays a bounded history window across joined rooms. Previously observed
messages can recover newly added mentions using saved bounded evidence; unknown or
evicted old evidence cannot establish a mention transition. Retention/budget gaps are
visible. Concurrent reaction edits from another connection still have no protocol CAS. Targets evicted by history bounds require manual pagination.
See README for limits, clipboard upstream constraints and profile concurrency.
