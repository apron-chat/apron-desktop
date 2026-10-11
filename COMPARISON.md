# Architecture and performance evidence

This standalone alternative uses Rust and GPUI Kit 0.7.1 for native Wayland
windows and a virtualized variable-height transcript. UI-independent model,
authentication and transfer modules communicate through bounded channels with a
two-worker Tokio runtime. Shared message records and list splices avoid rebuilding
the entire history for every update. No browser or GTK application is embedded.

The tradeoffs include the Rust dependency/build stack, Vulkan/driver requirements,
and platform integration still requiring real NixOS/niri validation. This is a
working experiment, not complete web-client parity or a performance-superiority
claim. The implementation choice belongs to the maintainer; the two draft PRs
are independent alternatives against main.

See [DAILY_USE.md](DAILY_USE.md) for the latest measured implementation pass and
[STANDALONE.md](STANDALONE.md) for the root-layout validation. Historical GPUI
reports remain in `docs/`; they record their original paths and test environment.
The benchmark harness is `src/benchmark.rs`, with an optional standalone synthetic
seeder in `scripts/seed_reference.py`.

The daily-use run used 1,000 fictional messages under Weston 14 and software
Vulkan (llvmpipe), measuring 147.2 MiB RSS and no CPU ticks or row renders during
the settled 10-second interval. Latencies shared CPU with build work. These are
individual warm observations, not controlled comparative results or GPU frame
presentation measurements. Zero ticks means below measurement resolution,
not zero energy use. Hardware GPU, niri, IME and assistive software remain untested.

[Raw benchmark](docs/daily/benchmark.json) · [Native design review](DESIGN_REVIEW.md)
