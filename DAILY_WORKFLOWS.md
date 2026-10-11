# Native daily workflows

## Architecture and scope

Rust, GPUI and GPUI Kit render directly through Wayland/Vulkan. A bounded Tokio
actor owns protocol I/O; the UI virtualizes retained history. This standalone
implementation shares no runtime or state with the alternative GTK client or web
client. The choice keeps native text/input and one application binary without a
browser; GPU/toolkit portability still requires real desktop testing.

This milestone adds versioned per-server/user appearance, notification and privacy
preferences; bounded file drop/clipboard staging; a searchable Unicode reaction
picker; advertised optional presence and opt-in idle; and attachment title editing
both before sending and in the author's sent-message editor. Titles are independent
of captions and preserve embed identity/URLs. No paste or drop automatically sends.

Preferences never contain credentials. Draft/read persistence remains separately
opt-in; disabling it retains only preferences, and Reset removes the profile.
Existing profile bounds, private atomic writes and ordered reset semantics remain.
File offers are at most four regular files/8 MiB total, with cancellation and stale
result rejection. URI clipboard reads are bounded to 64 KiB and three seconds.
Native image paste and ordinary text paste retain their existing paths.

Reaction choice uses the existing complete-own-set preflight, including retention
gap refusal; a picker does not weaken reconciliation. Presence offers only values
advertised by the server. Idle is off by default and uses app focus/input plus a
one-shot five-minute timer, not global desktop input or busy polling. Current Go
server boolean-idle compatibility is enabled only by its exact type rejection;
modern numeric, legacy and denied cases are tested.

## References

- Protocol repository: `3df659ac821ec67554fc0cdbf698842758d15928`.
- Web reference: `e09522d` (attachment display-title behavior included).
- Fresh official Go test server: `067083815be6b113e7d35f6569fa35dee0052fd5`.
- Pinned application dependencies: Cargo.lock and flake.lock. Nix adds wl-clipboard
  to the runtime wrapper for explicit file clipboard offers.

## Verification

All test data is fictional. Test runners own their temporary compositor, in-memory
servers, upload directories and XDG state, refuse occupied ports and clean up only
their own processes. No production connection, bot, credentials or real chat data
was used.

- Cargo release build, formatting and core all-target Clippy with warnings denied passed.
- 49 core/fixture tests passed; eight opt-in loopback tests passed against the fresh
  official server plus a synthetic fixture. Core CI intentionally ignores those
  eight server tests; they were run separately here.
- Native daily suite: 19 checks at 640×480 and 1280×900, including PNG/JPEG image
  paste, actual Wayland URI-list clipboard, explicit uploads, shared title changes,
  preference reload/reset, Unicode reactions, idle transitions, cancel/errors and
  stale edits. See the committed reports and exact binary hashes below.
- Final 640×480 synthetic native smoke covers selection/copy, pagination anchor,
  rapid room switches, keyboard reply, interrupted send and permission errors.
- Exact-head GitHub core/Nix status is recorded in PR #2. The previous baseline
  `46e19b5` passed both jobs. There is no local Nix executable.

The drop test dispatches native GPUI FileDropEvent events. It verifies the actual
application drop path and pointer-modality adapter, but does not simulate a real
file manager's compositor data offer. URI clipboard testing does use wl-copy and
wl-paste through the owned Wayland compositor.

## Efficiency sample

One isolated 1,000-message software-renderer run retained 549 messages/664,290
serialized bytes at 188.84 MiB RSS. The settled ten-second interval used 0.02 CPU
seconds (~0.20% of one core) and made zero row-renderer calls. The preceding
settling interval used 1.93 CPU seconds and 52 row renders; startup was 786 ms.
This is one sample, not a statistical or hardware performance claim. The report
records scheduling/layout delays separately and carries the same release hash
as the native suites.

## Visual review

Minimum-size screenshots exposed overly fixed dialog heights; body height now
follows the viewport, action footers remain visible, and storage details use a
disclosure. Both title inputs and Save fit at 640×480. Native dark preferences and
light title editing were inspected, including readable state/error labels.

[Minimum preferences](docs/daily-workflows/minimum/report-privacy.png) ·
[Minimum sent-title editor](docs/daily-workflows/minimum/report-edit-attachment-title.png) ·
[Wide emoji picker](docs/daily-workflows/wide/report-emoji-picker.png) ·
[Wide file staging](docs/daily-workflows/wide/report-file-attachments.png).

## Limits and next work

Real niri/hardware rendering, file-manager drag offers, Secret Service round-trip,
notification daemon delivery, actual email provider, IME and assistive software are
unverified. Passkeys, avatars, larger/media playback and broader server-specific
moderation remain unimplemented; [the integration assessment](INTEGRATION_ASSESSMENT.md)
separates feasible local work from platform/product decisions. No complete archive
reaction/unread census or remote text search is claimed. See README for memory,
profile concurrency and upstream image clipboard buffering limits.
