# Conversation refinement

This pass uses [Swiftcord's public site](https://swiftcord.app/) and the
[official project screenshot](https://github.com/SwiftcordApp/Swiftcord/blob/v2/docs-res/hero.png)
as design references. Transferable patterns are a clear workspace hierarchy,
compact author rhythm, restrained contextual controls, and an integrated composer.
Apron keeps its own identity and Linux keyboard conventions. There is no copied
branding, server rail without a multi-server feature, glass blur, or new animation
polling loop. Toolkit menus/dialogs keep their normal transitions.

## Implemented design

- A tinted workspace surface, local Apron monogram, stronger selected-room contrast,
  nested threads, and separate All rooms / Unread controls.
- Deterministic local author initials and color accents, five-minute same-author
  runs, separate reply headings, quieter timestamp hierarchy and hover/focus actions.
  Row identity and text selection remain native; structural changes remeasure
  affected grouping boundaries to preserve list anchors.
- A rounded, integrated composer with accessible attachment/mention/preview controls,
  an accent Send action, a distinct reply strip and compact removable file cards.
- Coordinated opaque light/dark colors, consistent borders/radii and muted labels;
  a session theme switch in the conversation menu. Compact navigation stays behind
  the existing toggle, and focus rings remain visible.
- A scoped search strip with count, previous/next controls and a highlighted result.
  Matching work happens when the query or retained data changes, not on every row
  render. Ctrl+F opens it; Enter/F3/Shift+F3 navigate; Escape from its input closes it.

## Added daily-chat workflows

Reactions use Apron 8's advertised `reactions` RPC and per-user complete replacement
sets. The message submenu offers four common emoji. Existing chips show totals and
whether your known set contains the reaction, and can toggle it. Broadcast/history
records are ordered by log ID; empty sets clear, duplicates collapse, tombstones hide
chips, and stale records cannot resurrect a cleared set. Requests are serialized,
server errors are visible, and uncertain requests are never automatically retried.
The official-server test covers replacement, removal and later changes while
reconciling an earlier history page.

Search is deliberately labeled “loaded only”: current-room retained message text,
not remote history or all rooms. Manual pagination expands the retained evidence.
The Unread room filter uses existing bounded attention. Full archive reaction counts,
unseen/evicted own reaction sets, arbitrary emoji picking, room administration,
notifications, typing/presence, remote search, and durable theme preferences remain
outside this pass. See FEATURE_PARITY.md and README.md for full limits.

## Visual review and verification

Evidence is in `docs/refinement/`. Native captures use fictional loopback data.
The first pass revealed empty reaction containers adding vertical space and overly
tall attachment cards hiding later previews. The final pass removes empty containers
and fits four staged files into a compact list. Search integration testing caught a
programmatic input synchronization issue; the observer now updates the cached results.

The native harness checks real GPUI input/menu dispatch, Wayland selection/clipboard,
scroll anchoring, search, reactions, reply/copy, keyboard focus, unread activity,
masked-token dialogs, and compact states. Daily-use checks connect the transport
actor to a separately built official Apron server with memory-only data and temporary
uploads. `scripts/native-check.py` refuses occupied ports, launches its own isolated
Weston, uses fresh private state/cache directories, and cleans up only its children.
Its `--debug` compositor is for this isolated test session only.

Final run results and the paired software-renderer benchmark are recorded below.
Full Nix builds, actual niri/hardware GPU, aarch64 compilation, IME and assistive
technology remain unrun. The unchanged pinned Nix development shell's previous
evaluation is documented in STANDALONE.md; it was not rebuilt for this visual pass.

### Repeating native checks

From a shell with the app's build/runtime libraries, Weston development headers,
`weston-screenshooter`, `wl-clipboard`, D-Bus and Python `websockets` available:

```sh
# This workspace uses Weston 14; match the ABI to your installed compositor.
cc -shared -fPIC $(pkg-config --cflags libweston-14) \
  scripts/weston-test-seat.c -o /tmp/apron-test-seat.so \
  $(pkg-config --libs libweston-14)
python3 scripts/native-check.py --seat /tmp/apron-test-seat.so \
  --mode polish-smoke --output /tmp/apron-ui-check
python3 scripts/native-check.py --seat /tmp/apron-test-seat.so \
  --mode daily-smoke --aprond /path/to/separately-built/aprond \
  --output /tmp/apron-daily-check
```

Use new output directories for every run. `--width`, `--height`, `--binary`,
`--weston`, `--shell` and `--screenshooter` can select installed components.
`--mode smoke` verifies selection/scrolling; `--mode benchmark` seeds 1,000 synthetic
messages into its owned official server. Software Vulkan can be selected using
`VK_DRIVER_FILES`. The runner never supplies real credentials or remote URLs.
In this workspace `scripts/workspace-env.sh` supplies the extracted native libraries;
Weston binaries/modules are separately supplied explicit paths. The runner records
its input binary's SHA-256 in `run.json`.

### Regression check

Sequential release runs used the same 1,000-message / 1,045-byte synthetic seed,
1280×900 headless Weston 14 and Mesa llvmpipe selected by `VK_DRIVER_FILES`.
The baseline was built from `02050a0446704d27ca4382de9ebc4cec56d46bf9`.
No other native test clients or builds ran during these two recorded runs. Each
had fresh private state and shader cache; the harness's historical “warm run”
label refers to paging in an already-started process, not a warm shader cache.

| Measurement | Previous GPUI | Refined GPUI |
|---|---:|---:|
| RSS after settling (MiB) | 170.17 | 170.48 |
| Startup to first history (ms) | 953.03 | 963.93 |
| Median older page (ms) | 37.98 | 42.22 |
| Slowest older page (ms) | 49.19 | 76.18 |
| Maximum view description (µs) | 214.00 | 1046.00 |
| Total row renderer calls | 102.00 | 138.00 |
| Settled 10-second CPU time (s) | 0.00 | 0.00 |
| Settled row renderer calls | 0.00 | 0.00 |

Both retained 549 messages / 664,290 serialized bytes. Memory differs by less
than 1 MiB and settled idle observed no CPU ticks or row rendering in either run.
The refined view still performs more row rendering and its worst measured page
and view-description times are higher. Narrowing history remeasurement reduced
the intermediate refined run from 150 to 138 row calls; it did not eliminate the
additional work. These single software-renderer samples do not establish a speed
advantage or a statistically reliable latency regression. View-description timing
excludes layout/paint; the startup heartbeat includes first-render stalls. Hardware
GPU profiling and repeatable multi-run latency measurement remain future work.

### Final validation

All six final native runs used release binary SHA-256
`9acae85dea2d809acab5b61c672025634ae7230159c6115fa54829cbd41bb3b8`.
The official server was separately built from Apron commit
`067083815be6b113e7d35f6569fa35dee0052fd5`.

| Check | Result / evidence |
|---|---|
| Formatting and release Clippy (`-D warnings`) | Passed; [Clippy log](docs/refinement/refine-clippy-final.log) |
| Release build | Passed; [build log](docs/refinement/refine-build-final.log) |
| Core/auth/media/grouping tests | 28 passed; [test log](docs/refinement/refine-tests-final.log) |
| Official/fixture integration tests | 5 passed; [integration log](docs/refinement/refine-loopback-final.log) |
| Desktop 1280×900 | Passed; [report](docs/refinement/desktop/report.json) |
| Compact 800×900 | Passed; [report](docs/refinement/compact/report.json) |
| Minimum 640×480 | Passed, including enlarged type; [report](docs/refinement/minimum/report.json) |
| Native selection / external clipboard / prepend anchor | Passed, 0 px drift; [report](docs/refinement/selection/report.json) |
| Official-server daily-use workflows | Passed; [report](docs/refinement/daily/report.json) |
| Test runner safeguards | Both refuse occupied listening ports; owned process cleanup and fresh private state |

The daily-use test includes two real loopback uploads, explicit image loading,
PNG/JPEG native clipboard offers, text paste, author-only edit/delete, stale edit
rejection, explicit mention IDs, reaction add/remove through the transport actor,
and opt-in local-state save/reset. No private chat or production credentials were
used. Emoji appearance depends on installed fonts; these captures use monochrome
fallbacks. Actual niri, hardware rendering, IME, screen readers and full Nix builds
are still not verified.

### Current native captures

[Wide light](docs/refinement/desktop/report-wide.png) ·
[Wide dark](docs/refinement/desktop/report-dark.png) ·
[Compact](docs/refinement/compact/report-wide.png) ·
[Minimum](docs/refinement/minimum/report-wide.png) ·
[Search](docs/refinement/desktop/report-search.png) ·
[Reactions](docs/refinement/desktop/report-reactions.png) ·
[Attachments](docs/refinement/daily/report-image-preview.png) ·
[Sign-in error](docs/refinement/minimum/report-sign-in-error.png)
