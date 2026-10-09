# Verification — 2026-10-09

Executed in the saved Debian 13 workspace using isolated, locally extracted
libraries. Existing apron-web checkout remained unchanged. No production
server, credentials, bot state, or real chat content was used.

## Actual results

- Python syntax compilation: passed for all application modules and fixture.
- Pure reducer/rendering tests: **11 passed**.
- Real libsoup WebSocket integration tests: **4 passed** against the local
  fictional protocol-8 fixture. Covered guest auth, token rotation, invalid
  token stopping without guest fallback, listings, pagination, reply/mention
  encoding, thread creation, denied sends, disconnect uncertainty, and reconnect.
- Native GTK workflow smoke test: **passed at 1280×900 and 640×900** under
  headless Weston. Drove actual widgets through room/thread navigation,
  backward pagination, per-room drafts, a rejected send with draft retention,
  a confirmed reply, and screenshots. No Python callback exceptions in these runs.
- Screenshots captured through GTK's renderer on a Wayland display and visually
  inspected: `docs/preview.png`, `docs/preview-compact.png`. Sidebar is visible
  at desktop width and collapses at compact width. Composer remains available.
- Official Go reference interoperability: **1 end-to-end test passed**, separate
  from the 4 fixture tests above. Built the unchanged saved official checkout
  `067083815be6b113e7d35f6569fa35dee0052fd5` with Go 1.27.1. The server identified
  as `apron-go/8` and ran only at `127.0.0.1:18766/ws`, with `--store memory`,
  a dedicated temporary upload directory, and synthetic data. Tested guest
  connection/auth, listing, sending/broadcast-before-confirmation, a reply with
  an explicit mention, thread creation and thread history, backward pagination
  over 65 posts, client-initiated disconnect, automatic reconnect, and reloading
  message history. No application compatibility defect was found in these flows.
  The Go guest policy can assign a fresh identity on reconnect: this is **not**
  evidence of official-server token-resume interoperability.
- After enlarging/darkening metadata and replacing the reply glyph with a
  labeled button, all 15 fixture/unit tests and both GTK sizes passed again.
  The compact GTK workflow passed again after the redundant-render optimization.
- Python wheel built successfully with `pip wheel --no-deps .`.

Test libraries: Python 3.13 (system interpreter), GTK 4.18.6,
libadwaita 1.7.6, libsoup 3.6.5, Weston 14.0.2 (headless + pixman),
markdown-it-py 3.0.0, websockets 15.0.1.

Commands used from `/workspace`:

```sh
GSETTINGS_BACKEND=memory XDG_CACHE_HOME=/tmp/apron-cache \
  apron-native/scripts/local-env.sh /usr/bin/python3 \
  -m unittest discover -s apron-native/tests -p 'test_*.py' -v

XDG_CACHE_HOME=/tmp/apron-cache GSETTINGS_BACKEND=memory \
XDG_RUNTIME_DIR=/tmp/apron-wayland WAYLAND_DISPLAY=apron-wayland \
GDK_BACKEND=wayland GSK_RENDERER=cairo \
  apron-native/scripts/local-env.sh dbus-run-session -- /usr/bin/python3 \
  apron-native/tests/ui_smoke.py /workspace/apron-native/artifacts/preview.png
```

The compact run used `WAYLAND_DISPLAY=apron-wayland-compact`, a second
headless Weston output at 640×900. Weston used `--backend=headless
--renderer=pixman --shell=…/kiosk-shell.so --idle-time=0 --no-config`;
`WESTON_MODULE_MAP` located the extracted headless backend.

## Not run / remaining validation

- Nix flake evaluation/build (Nix is not installed here); niri-specific launch.
- The full reference-server conformance suite, official-server token auth/resume,
  and server-process-restart recovery. Official guest flows above were tested;
  fixture token tests remain distinct.
- Real TLS endpoints, passkeys/email, long-duration load/soak, accessibility
  assistive-technology audit, and hardware/GPU-specific scaling.

The project is an early usable vertical slice. See README limitations for
incomplete recovery and optional protocol features; passing these focused tests
does not establish full protocol conformance.

## Synthetic efficiency measurement

Actual GTK widgets on headless Weston, 1280×900, Cairo software rendering;
libsoup connected to the official Go server above. Seeded a separate public
room with 1,000 synthetic messages, each with 1,045 UTF-8 text bytes. Opened the
room's latest 50-record page, then 10 older pages. The server's record limit
includes membership changes, so it need not return exactly 50 messages.

After the quality-pass optimization:

| Measurement | Observed |
|---|---:|
| Startup to initial room history | 137.9 ms |
| Benchmark-room latest page | 524.0 ms |
| 10 older pages, minimum / median / maximum | 501.3 / 545.8 / 779.5 ms |
| Worst 10-ms heartbeat interval during loading | 735.4 ms |
| Idle wall / process CPU | 9.911 s / 0.0015 s |
| Idle CPU, percentage of one core | 0.016% |
| Client RSS / peak RSS | 75.6 / 76.2 MiB |
| Cached messages / serialized bytes | 587 / 678,989 |
| Rendered messages | 50 |

Timings include history handling, widget updates, and draining the GTK main
context; explicit 50/100-ms settling pauses are subtracted. The heartbeat is a
10-ms GLib timer removed before idle measurement. Idle uses GLib's blocking
main loop, process CPU time, and `/proc/self/status` RSS; the measurement does
not count the server or compositor. Raw data: `docs/benchmark.json` and
`docs/benchmark-before.json`. Harness: `tests/benchmark_ui.py`; synthetic seed:
`scripts/seed_reference.py`.

Before removing the redundant nonempty-page rebuild when loading begins,
older pages took 988–1,362 ms and worst heartbeat interval was 738.7 ms.
The optimization improved total page-load time, but **did not eliminate long
main-thread stalls**. Long-message pages still require row virtualization or
incremental layout. This is one warm local sample, not a throughput or soak
benchmark, and it cannot establish comparative efficiency against Rust,
apron-web, hardware-accelerated rendering, or real niri sessions.
