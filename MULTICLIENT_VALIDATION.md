# Synthetic multi-client validation

This follow-up closes the local account/role and multi-client protocol gaps left
by MEDIA_MODERATION.md. It uses official Go server `0670838`, protocol `3df659a`,
and synthetic data only. No production accounts, SMTP, bot state or credentials
are needed. Passkey contracts and audio codecs are outside this change.

## Fixes

- Fresh RPC operations now use a random 128-bit connection prefix plus a checked
  counter. Apron deduplicates request IDs per user across connections: starting
  each connection at `1` caused real same-account requests to collide. A core
  regression exercises three connections under a user-scoped deduplicating server.
  Uncertain writes are still never automatically retried.
- Reconnect room-list snapshots retain the bounded presence status alongside
  the existing bounded identity/avatar fields. Previously that snapshot discarded
  status until another presence notice arrived.
- Mutes now follow all retained thread ancestors, with a fixed traversal bound
  for malformed cycles. Previously only the immediate parent was checked. Core
  fixtures cover nested inheritance, expiry, global mutes and cyclic input.
- The actor and tests share the one-way typing sender. Activity does not have an
  RPC success response; waiting for one is incorrect.

## Isolated official-server checks

The runner owns ports 18765–18767 and refuses occupied ports. The third server
uses the supported `--role moderator=moderator@fixture.invalid` grant and
`--email.enable --email.sender log`, with passkeys disabled. Synthetic email codes
are consumed from a private temporary log, never printed or saved in evidence;
tokens remain in memory. All fixture files and child servers are cleaned up.

- Two connections of one account aggregate attended/idle presence correctly.
- Observer visibility covers online, idle, DND, invisible and opt-out, while the
  account's other connection receives private own-status updates.
- Private-room typing start/stop reaches a member, carries the actual sender ID,
  and does not reach an outsider.
- Room/global mutes synchronize across account connections, remain private from
  other users and return on token-authenticated reconnect.
- An actual locally granted moderator can kick and remove another member;
  ordinary members cannot. Removal prevents subsequent sends; reinvitation
  restores access. A kick reason appears in room history.
- Guests cannot self-grant roles and clients cannot override configured grants.
- A private-room outsider cannot add/remove members. Joined ordinary members
  **can invite** under this server's policy; this is not moderator-only.

## Recorded results

54 core/fixture tests, 11 opt-in official/fixture integration tests, and core
all-target Clippy with warnings denied passed. See `docs/multiclient/core.txt`,
`loopback.txt` and `clippy.txt`. The 11 integration tests are intentionally ignored
by ordinary cargo test; the isolated runner executes them explicitly.

## Desktop checks and limits

The native daily check now connects a second transport actor and checks DND after
reconnect, typing natural expiry and explicit stop, scoped/global mute on/off,
and timed mute expiry. Both 640×480 and 1280×900 runs passed all 23 check groups;
a separate 640×480 keyboard/selection/reconnect smoke passed. Reviewed compact
and wide light/dark screenshots retain visible viewer actions and moderation
errors. Results and previews are recorded in `docs/multiclient`.

The release build with `ui-tests` support passed. All three native runs used
binary SHA-256 `d144bb05850741bbbd15edd2bd66f04c988597d9b27e08b3e30a9d888826602c`.
Formatting and diff checks passed. Exact-head CI results are tracked in PR #2;
CI also builds the normal default-feature UI through Nix.

Actual niri/GPU behavior, real file-manager drag offers, portal Save selection,
Secret Service storage, notification-daemon delivery, SMTP delivery, IME,
assistive technology and aarch64 remain environment checks. Headless Weston and
the development email sender do not establish those results. Native passkeys
still require the origin/RP decision in PASSKEY_DESIGN.md; audio/video playback
still requires a native playback/codec decision.
