# GPUI experiment: local comparison

Keep GTK as the primary client for now. This GPUI vertical slice demonstrates
that a shared bounded message cache plus GPUI Kit's virtualized MessageScroller
can substantially reduce page-update work. It does **not** yet demonstrate a
more efficient whole application: the available software-Vulkan run used more
memory and completed some rendering after the page-load timing ended. A second
ten-second idle interval was quiescent. Scheduling stalls varied sharply between
the first and final warm runs.

## Architecture decision

Rust + GPUI Kit 0.7.1 is a reasonable controlled experiment for a GPU-rendered,
variable-height chat transcript. The toolkit supplies actual native Wayland
windows, text selection, textarea, buttons, and a virtualized message scroller.
The protocol/model/transfer modules are UI-independent. A separate two-worker
Tokio runtime owns WebSocket RPC and bounded asynchronous UI channels; HTTP file
transfers do not block WebSocket input. The UI retains shared `Arc<Message>`
records, changes list structure with splice operations, and renders visible rows.

The tradeoff is a substantially heavier Rust dependency/build stack than the
existing Python/GTK implementation, GPU/driver dependencies, and a less mature
integration surface. The code is isolated in `gpui/` on a local experiment branch.
This work does not alter the GTK source, feature branch, draft PR, or main. Nothing from this
experiment has been pushed or published.

## Local measurements

Both clients used the same freshly seeded, official in-memory Apron server,
1,000 fictional messages with 1,045 text bytes each, the same 1280×900 isolated
Weston 14 headless compositor, and the same machine. GPUI used an optimized release
build with Mesa 25.0.7 llvmpipe Vulkan; GTK used Cairo. No hardware GPU was exposed.
Compilers were idle during these sequential runs. Both cached 549 message records
after latest plus ten earlier history requests (the protocol limits log changes,
not final compacted message count).

| Measure | GPUI first run | GPUI final warm run | GTK baseline |
|---|---:|---:|---:|
| Latest benchmark page | 348.8 ms | 46.8 ms | 743.8 ms |
| Median of ten older pages | 69.1 ms | 38.6 ms | 646.6 ms |
| Worst interval of a 10 ms UI heartbeat during load | 462.0 ms | 57.9 ms | 64.3 ms |
| RSS after loading | 144.8 MiB | 144.8 MiB | 80.1 MiB |
| CPU during first ~10 seconds after loads | 16.50% | 18.00% | 0.049% |
| CPU during a second settled 10 seconds | Not measured | 0 ticks (<0.1% resolution) | Not separately measured |
| Cached records | 549 | 549 | 549 |
| Transcript records exposed to list | 549 | 549 | 50 |

The final source avoids cloning raw JSON for each visible row and includes the
keyboard/heartbeat corrections and UI polish from workflow testing. GPUI made 120 total
row-renderer calls during the final benchmark, including page loads and two
programmatic scroll jumps (96 in the first run). This confirms virtualization
rather than building 549 row trees on every update. GTK deliberately exposes a bounded
50-message page at a time, so the UI workloads are similar but not identical.
GPUI's final measured view-description maximum was 0.820 ms; that excludes layout,
painting, driver work and presentation and should not be advertised as frame time.

These are **individual warm observations**, not a statistical benchmark. The
later GPUI runs also benefited from warmer caches and cannot isolate the effect
of the code changes. All three runs are retained to show the variability. Scheduling
and software-renderer costs vary; neither renderer is running on the user's
actual hardware/niri setup. The UI heartbeat is an event-loop responsiveness
proxy, not GPU presentation latency or input-to-photon latency. CPU percentages
are process CPU, not machine-wide CPU; no hardware GPU utilization was measurable.
Startup timing begins after window/view construction and excludes cold shader
compilation/toolkit initialization, so it is not a cold process startup benchmark.
The initial GPUI window took noticeably longer while shader caches warmed.

The first ten-second post-load interval still performed 42 row renders and used
1.8 CPU seconds across the process. The next ten seconds performed **zero row
renders and zero additional `/proc` CPU ticks**. With 100 ticks/second, this is
below 0.1% of one core at this measurement resolution, not proof of literally
zero cost. GTK's finer-resolution process CPU measurement was already low in its
first interval. Do not describe GPUI's first interval as sustained idle CPU.

This also limits the page-latency interpretation: receipt/state-update timers can
finish before all deferred layout/rendering work is presented. GPUI's apparent
page-load advantage is promising but is not an input-to-presentation comparison.

Raw reports: [GPUI polished / settled idle](docs/gpui-benchmark-steady.json),
[GPUI intermediate](docs/gpui-benchmark-final.json),
[GPUI first](docs/gpui-benchmark.json), [GTK](docs/gtk-benchmark.json).
The harness is `src/benchmark.rs`; the GTK harness remains `tests/benchmark_ui.py`
at repository root. It intentionally mirrors page count, pauses, heartbeat period,
and initial idle duration; GPUI also has a second settled interval. Do not run
these against real chat content.

## Verification and limits

- Optimized x86_64 Linux build: passed with pinned Rust 1.99.0.
- Nine core/media tests: passed (bounded state, stale log changes, ordered splices,
  untrusted text sanitization, origin policy, redirects/auth forwarding, real file
  bounds including chunked downloads, serialized-byte cache eviction and
  no-overwrite atomic saves).
- Two opt-in loopback tests: passed against the official Apron server and fictional
  fixture (guest/list/create, 65-message pagination, reply, two file uploads and
  exact-byte downloads, single-use write URLs, reconnect and interrupted actor).
- Native Wayland: actual Vulkan window rendered; screenshot visually inspected.
  `docs/preview.png` (1280×900) and `docs/preview-narrow.png` (800×900)
  contain fictional fixture messages only. Compact windows automatically hide
  the sidebar; Rooms or Ctrl+B toggles it. Messages show local-time timestamps
  and replies show a readable excerpt when the target is cached.
  `docs/preview-offline.png` shows a real refused loopback connection with
  reconnect state, retained local drafts and disabled network actions.
- Native workflow: passed selection, selection preservation across history prepend,
  zero-pixel anchor shift, stale response rejection, rapid room switches, keyboard
  reply, Escape cancellation, navigation toggle, room drafts, permission errors
  without reconnecting, interrupted send and retained drafts. Copy was verified
  by a **separate `wl-paste` Wayland client**. See `docs/gpui-smoke.json`.
- `cargo clippy --release --locked --all-targets -- -D warnings`: passed.
- Nix development shell: `flake check --no-build --all-systems` evaluated both
  x86_64-linux and aarch64-linux. Full Nix builds and aarch64 builds were not run.

Weston's headless backend initially failed GPUI startup because it provided no
`wl_seat`. A tiny test-only module, with signatures verified against Weston 14.0.2
source, supplies a virtual seat. It has no hardware access and does not change the
host desktop. This is a test-environment workaround, not an application patch.
Native smoke input is injected through GPUI's real window event dispatcher, not
through physical devices or a compositor input protocol. GPUI platform source
requires a compositor-issued keyboard/pointer press serial for clipboard ownership;
our injected events do not provide one. The opt-in test seat supplies one genuine
compositor Shift press/release for the clipboard check, then stops. This was not
enabled during the original performance runs; the callback was already finished
before the final run. Real niri integration remains unverified. The suite verifies widget and
application paths but is not evidence for hardware input/IME behavior.

The first selection smoke failed because all pointer events were dispatched in
one turn without hover/layout settling. Realistic separated events fixed it.
The copy test initially compared a trailing newline against GPUI Kit's deliberately
trimmed copy output; the corrected expectation and independent clipboard reader
both pass. The keyboard smoke exposed Textarea's default secondary-enter newline insertion;
the app overrides that shortcut instead of weakening the expected reply text.

File chooser portals were not available end-to-end in this isolated environment;
the file transfer protocol and destination safety are tested independently.
Rich attachment previews, native IME, accessibility assistive software, fractional
scaling, hardware GPU, and actual niri behavior remain unverified. Selection
spanning unrendered rows and resize-during-selection behavior also remain unverified.
Unread/mentions, token auth, persistence, notifications, and moderation are intentionally
outside this comparison's scope; see the complete list in README.md.

## Next decision

Keep GTK primary until feature parity and real NixOS/niri GPU validation. GPUI
is a credible direction for a continuous long transcript: selection, anchor
preservation, keyboard flows and settled idle all behaved well here. Next measure
actual frame presentation on hardware, verify IME/accessibility and file portals,
and run repeated like-for-like trials. GTK currently uses less measured memory
and provides the broader implemented product feature set.
