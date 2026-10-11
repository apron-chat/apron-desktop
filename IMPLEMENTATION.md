# Implementation checklist

Baseline: desktop 0caa555, apron-web e09522d, Dissent 6ff6182.
Status: Planned → Implemented → Tested; Blocked means a concrete dependency is recorded.
Tests alone never establish full protocol or web parity. Synthetic local servers only.

| Milestone | Status | Acceptance criteria |
|---|---|---|
| Reaction correctness | Tested | Unknown differs from empty; reconcile complete own set before replacement; refuse incomplete/expired/budget-exceeded history and mid-scan retention gaps; rejection/interruption never retries a write |
| Attention recovery | Tested within bounds | Bounded reconnect replay with retained log watermarks, offline edits/mentions deduplicated; gaps explicitly visible; persisted cursors do not invent complete offline counts |
| Design foundation | Native light/dark/gallery review passed | Shared theme/spacing/control/focus rules; GPUI Kit controls; live component/state gallery; light/dark screenshots |
| Compact composer | Native wide/compact/minimum passed | One-line idle, bounded growth, reply/files remain visible, keyboard/selection unaffected |
| Navigation | Native switcher/keyboard passed; disclosure implemented | Collapsible parents preserve hidden unread/mentions; fuzzy keyboard room/thread switcher; narrow selection returns focus to composer |
| Room/member workflows | Official-server room and two-user membership tests passed | Protocol-backed create room/thread, settings, leave, member list/add/remove; errors retain input; no automatic retry of uncertain writes |
| Presence/typing/mutes | Official multi-client tests passed | Capability-gated bounded event state; expiring typing without idle busy polling; protocol-supported controls |
| Notifications | Implemented; daemon delivery unverified | Explicit opt-in, no unsolicited message body disclosure, mute/focus respected; unavailable service visible |
| Secure login | Implemented; missing-helper test passed, real keyring unverified | Optional Secret Service keyring; no plaintext fallback; rotation handled; errors do not expose tokens |
| Email/passkeys | Official dev-sender email account tests passed; passkeys not implemented | Match advertised server schemes; document concrete platform/provider blockers rather than approximate credentials |
| Links/media/search | Safe link action tested; retained search/media preserved | Explicit safe link actions; bounded media; search scope honest; unsupported remote search not invented |
| Nix/desktop/CI | Baseline core and Nix CI passed; exact-head results in PR #2 | Installable package/app plus .desktop/icon; locked CI tests; distinguish evaluation/build/hardware checks |
| Final validation | Core/official/native and final delivery smoke passed | Native wide/compact/minimum/light/dark/forms/errors/menus; regression tests; performance bounds; reviewed screenshots |

Optional conversation tabs are deferred until core workflows are stable. Discord guild rails,
branding and assets are not part of the design. Real niri, hardware GPU, IME and assistive
software require an environment that exposes them; headless tests cannot certify them.

## Bounds and remaining work

- Recovery scans at most 32 pages of 50 changes across joined rooms, interleaving
  foreground requests and heartbeat work. Persisted attention retains at most 2,000
  message/log-ID and mention-flag records plus 2,000 pending IDs. Unknown or evicted
  old mention evidence cannot establish an edit transition. No complete archive claim.
- Reactions refuse writes when the bounded scan cannot prove the full own set. The
  protocol has no compare-and-swap for concurrent same-account reaction writers.
- Room lists retain 256 rooms. Protocol 8 has room/parent/activity filters but no
  generic pagination cursor or remote text-search method. Those are protocol constraints.
- Passkeys need an origin/RP-correct native WebAuthn/CTAP integration and authenticator
  testing. No browser-token extraction or unsafe approximation is substituted.
- Real Nix evaluation/build is blocked locally by the missing Nix executable. Real
  keyring round-trip is blocked by missing secret-tool/service; notification delivery,
  niri, hardware GPU, IME and screen-reader behavior need their actual desktop environment.
- Follow-up feature work (not claimed implemented): audio/video playback,
  native passkeys and broader moderation commands. See INTEGRATION_ASSESSMENT.md. Conversation
  tabs remain an optional later design experiment.

## Latest daily workflows

Durable versioned preferences, explicit file drop/clipboard staging, pre-send and
post-send attachment title edits, searchable Unicode reactions and advertised optional
presence/opt-in idle are implemented. See DAILY_WORKFLOWS.md for verification and limits.

Explicit bounded raster viewing, same-server/inline avatars with confirmed upload,
and confirmed server-specific /kick now have implementation and evidence in
MEDIA_MODERATION.md. PASSKEY_DESIGN.md records the unresolved origin/RP decision.

Synthetic multi-client presence, typing, private mute synchronization and actual
moderator-role permissions are covered in MULTICLIENT_VALIDATION.md.
