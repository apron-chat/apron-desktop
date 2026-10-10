# Standalone GPUI layout verification

The Rust application, Cargo files, pinned Nix development shell, tests and synthetic
fixtures now live at the repository root. No Python/GTK application, GTK packaging,
GTK assets or GTK tests remain in this branch's final tree. Python is only an
optional fixture/capture tool. The independent GTK alternative remains in PR #1.
This is a normal forward commit; shared historical commits are not rewritten.

Checks rerun from the new root:

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo test --locked --offline --no-default-features` | 25 passed; four opt-in integration tests separately run |
| `cargo clippy --release --locked --offline --all-targets -- -D warnings` | Passed |
| `cargo build --release --locked --offline` | Passed |
| release binary `--help` | Passed |
| `scripts/run-loopback-tests.py --aprond …` | All four integration tests passed |
| Native Wayland `--smoke` at 1280×900 | Passed under headless Weston 14 / software Vulkan |
| `nix flake check --no-build --all-systems` | Both x86_64-linux and aarch64-linux development shells evaluated |

[Logs and machine-readable native results](docs/standalone/) record these runs.
The optional workspace toolchain wrapper supplied extracted build libraries.
The integration runner built and used the official Apron server at
`067083815be6b113e7d35f6569fa35dee0052fd5`, memory storage, temporary upload
files and synthetic messages only. It owns and cleans up its test processes and
refuses occupied fixture ports. No GTK checkout helpers were used.

Native verification covered drag selection, external Wayland clipboard transfer,
zero-pixel prepend anchor movement, selection preservation, stale responses,
rapid room switching, keyboard reply/cancel/navigation, draft restoration,
permission errors and interrupted sends without automatic resending.

Existing screenshots and DAILY_USE/DESIGN_REVIEW evidence show the same GPUI
implementation before this layout-only migration (the window title now says
Apron · GPUI). Their original logs retain truthful historical paths. They are
not new screenshot captures or freshly rerun daily-use/benchmark suites.

Nix emitted a cache.nixos.org proxy HTTP 403 warning but evaluation completed.
A full Nix package build, aarch64 compilation, hardware GPU, actual niri, IME and
assistive technology tests remain unrun. The flake provides a development shell,
not an installable package. README and FEATURE_PARITY retain the protocol and
product limitations, including partial offline unread recovery and the upstream
clipboard reader's lack of a pre-read memory bound.
