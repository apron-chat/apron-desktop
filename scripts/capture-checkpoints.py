#!/usr/bin/env python3
"""Capture opt-in --polish-smoke checkpoints on an isolated Weston compositor.

Usage: capture-checkpoints.py /tmp/new-report.json -- weston-screenshooter
Inherit the same XDG_RUNTIME_DIR and WAYLAND_DISPLAY as the test client.
Never enable Weston's debug/screenshot protocol on an ordinary desktop.
"""
import pathlib
import re
import subprocess
import sys
import time

report = pathlib.Path(sys.argv[1]).resolve()
command = sys.argv[2:]
if command[:1] == ['--']:
    command = command[1:]
if not command:
    command = ['weston-screenshooter']
seen = set()
deadline = time.monotonic() + 120
checkpoint = pathlib.Path(str(report) + '.stage')
while time.monotonic() < deadline:
    if checkpoint.exists():
        name = checkpoint.read_text().strip()
        if re.fullmatch(r'[a-z-]+', name) and name not in seen:
            time.sleep(.45)
            previous = set(report.parent.glob('wayland-screenshot-*.png'))
            subprocess.run(command, cwd=report.parent, check=True, timeout=5)
            created = set(report.parent.glob('wayland-screenshot-*.png')) - previous
            if len(created) != 1:
                raise RuntimeError('Expected one new Weston screenshot')
            destination = report.with_name(f'{report.stem}-{name}.png')
            created.pop().rename(destination)
            print(destination, flush=True)
            seen.add(name)
    if report.exists():
        break
    time.sleep(.1)
else:
    raise TimeoutError('No completed native report within 120 seconds')
