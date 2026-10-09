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
- A full reference-server conformance suite or a live Go Apron server. Tests
  used a realistic wire fixture with actual WebSockets, not production services.
- Real TLS endpoints, passkeys/email, long-duration load/soak, accessibility
  assistive-technology audit, and hardware/GPU-specific scaling.

The project is an early usable vertical slice. See README limitations for
incomplete recovery and optional protocol features; passing these focused tests
does not establish full protocol conformance.
