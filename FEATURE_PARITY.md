# Apron web → native functional parity

Reviewed actual apron-web source at `8efacfbe241134bd414bad81c2d49ec3e7683913`,
particularly `docs/behavior.md`, `docs/architecture.md`, protocol `client.ts`,
`reducer.ts`, and the referenced UI components. This is an interaction benchmark,
not a claim of complete parity or a pixel-copy target. Status applies to the
current native checkpoint. “Implemented” still means the focused behavior,
not every policy or browser edge case of the web implementation.

| Web behavior / source evidence | Native status | Native affordance and remaining gap |
|---|---|---|
| Guest/token auth; rotating tokens (`client.ts`, behavior auth) | Implemented | Native connection dialog, memory-only default; token fixture-tested, official guest-tested. |
| Passkey login/register, WebAuthn Signal API (`webauthn.ts`) | Missing | Browser WebAuthn cannot simply be called from GTK. Need a deliberate native authenticator or browser-mediated integration; no insecure emulation. |
| Email sign-in/add-email, separate pending connection (behavior email) | Missing | Requires a cancellable two-stage auth UI and identity-switch tests. No email was sent in development. |
| Stored sessions (`storage.ts`, behavior auth) | Partial | Opt-in asynchronous libsecret Secret Service adapter and saved-session button. No plaintext fallback. Actual keyring service unavailable in test environment; adapter tests only. |
| Cached prior-session rooms/messages (behavior persisted view) | Missing | No durable chat cache, drafts, or read markers. Safe connection/theme preferences are persisted atomically instead. |
| Bounded history; reconnect checkpoint recovery (`client.ts`) | Partial | 50-record pages and bounded RAM; reconnect refreshes latest window. No complete durable checkpoint replay. |
| Core snapshots, edit/move/tombstones (`reducer.ts`) | Implemented | Greatest-log snapshots, embedded replies, moves, tombstones; memory-eviction limitations remain. |
| Current users/roles; identity alias reconciliation (behavior identity) | Partial | Complete/partial current-user updates, profiles; alias reconciliation and full role/status UI missing. |
| Rooms/threads; joined and browsable rooms (behavior room navigation) | Implemented | Searchable native sidebar, join action, parent shortcut, room/thread dialogs. |
| Room creation/title/description (`NewRoomDialog.svelte`) | Implemented | Native form, guarded pending submit, server-authoritative results. |
| Private rooms, inherited thread privacy (behavior private rooms) | Implemented | Explicit private creation and verify-private response before opening or replying. |
| Room edit/leave (`room_set`, behavior room menu) | Implemented | Room dialog and leave confirmation; private rejoin may require invitation. |
| Member list/add/remove (behavior members) | Implemented | Native room-details list, copy ID, exact-ID invitation, confirmed removal. Server remains permission authority. Truncated lists show counts. |
| Membership-log replay (`reducer.ts`) | Partial | Current member state replays by log ID. Timeline join/leave event grouping is not implemented. |
| Thread feed cards/summary positioning (behavior threads) | Partial | Threads in sidebar and summary in header; no interleaved thread cards or message-count previews. |
| Send/reply/per-room drafts (behavior composer) | Implemented | Native TextView, Ctrl+Enter, reply strip; drafts in RAM only. |
| Replying to an unjoined room/thread joins first (behavior threads) | Implemented | Join confirmation precedes sending; a failed join retains the draft and sends nothing. |
| Mentions/member suggestions (behavior composer) | Partial | Explicit member picker writes mentions; no inline chip/autocomplete editor. |
| Message edit/delete (behavior message actions) | Implemented | Native dialogs, confirmation, preserve client fields, reject stale local snapshot. Own-message UI only; no moderation actions. |
| Single-message move (behavior message actions) | Implemented | Destination picker preserves body/reply/ext. |
| Shift-click multi-select/bulk moves (behavior multi-select) | Missing | Requires per-message result/retry handling and selection UI. |
| Reactions/emoji picker (behavior reactions) | Partial | Per-user replay/counts and toggles, small picker/custom emoji; no full emoji catalog/autocomplete. |
| Copy message/code/user IDs (behavior copy) | Partial | Native clipboard message text/ID/member ID; no per-code-block copy button. |
| Navigation search/autocomplete (behavior mentions/#/emoji) | Partial | Room filter and bounded loaded-message search. General server-side full-text search was not found in the inspected web client or protocol; native cache search is an extra with explicit scope. |
| Browser back/forward and thread return (behavior navigation) | Partial | Alt+Up parent and Alt+N next unread; no full navigation-history stack. |
| Typing activity (behavior activity) | Missing | Requires throttled composer events and expiring per-room indicators. |
| Profile name/status (`ProfileBar`, `ProfileCard`) | Partial | Native profile/name and advertised status picker. Avatar upload, account linking, richer member status indicators missing. |
| Presence idle/mute/DND/preferences (behavior status) | Partial | Own status can be set. No idle reporting, timed mutes, or notification policy matrix yet. |
| Unread/mentions/read markers (`UnreadTracker`, behavior unread) | Partial | New live creations, focus/bottom read, own cursors; no missed-history recovery or edit-added mention alerts. |
| Desktop/page notifications (`notifications.ts`) | Partial | Opt-in Gio.Notification mentions while inactive, content-free body and room activation. Real desktop notification daemon not available for delivery testing. |
| Web push/service worker (behavior push) | Intentionally different | Native OS notifications while running; background push/closed-app wake not implemented. Need a Linux service design, not a browser service worker copy. |
| Uploads, media, file preview/download (behavior embeds) | Missing | Safe inert text fallbacks only. Native file chooser/portal, bounded transfer, provenance policy still needed. |
| Streaming/OpenGraph/trusted media (behavior embeds) | Missing | No remote resource fetch; security boundary intentionally conservative. |
| Markdown/code/tables (behavior formatting) | Partial | CommonMark → restricted Pango; common formatting, inert links/media. Rich tables and interactive links absent. |
| Commands (`commands.ts`, behavior commands) | Missing | Slash text remains ordinary text. Need capability-aware command controls and scoped notices. |
| Appearance (`PreferencesDialog`, behavior appearance) | Partial | System/light/dark via libadwaita; no custom typography/size/sliders. |
| Responsive panels/accessibility (behavior panels) | Partial | GTK focus, selectable text, larger labeled Reply, adaptive sidebar; no formal assistive-tech audit or resizable member panel. |
| Browser installation/update prompt (behavior PWA) | Intentionally different | Native desktop entry/icon and Nix packaging; distribution/package manager owns updates. |

## Current milestone and next work

The daily-chat milestone adds discoverable controls and focuses on correct
interrupted/repeated interactions. Focused official-server and native-widget
tests are in `tests/reference_interop.py` and `tests/ui_daily_smoke.py`.

Before claiming equivalent daily use, next priorities are:

1. Virtualized/incremental timeline layout and robust complete history recovery.
2. Actual Secret Service integration test, then email auth and a defensible
   native passkey path; account-scoped persisted cache/read state with privacy controls.
3. Typing/idle/mute, unread recovery, richer navigation and member status.
4. Bounded native upload/download/media handling and command/scoped-notice UI.
5. Multi-message workflows, full emoji discovery, and remaining polish.

No consequential product decision is required for the current safe defaults.
A passkey implementation choice will eventually require choosing an authenticator
integration; its security/portability tradeoff should be made explicitly.
