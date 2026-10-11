#!/usr/bin/env python3
"""Opt-in isolated native checks. Never attaches to an existing compositor/server.

Build weston-test-seat.c for your installed Weston ABI and pass --seat PATH.
The caller supplies native dependencies, Vulkan driver and Weston module paths.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--mode', choices=['smoke', 'polish-smoke', 'daily-smoke', 'benchmark'], default='polish-smoke')
    p.add_argument('--output', required=True, type=Path, help='New output directory')
    p.add_argument('--seat', required=True, type=Path)
    p.add_argument('--weston', default='weston')
    p.add_argument('--shell', default='kiosk-shell.so')
    p.add_argument('--screenshooter', default='weston-screenshooter')
    p.add_argument('--binary', type=Path, default=Path('target/release/apron-gpui'))
    p.add_argument('--aprond', type=Path)
    p.add_argument('--width', type=int, default=1280)
    p.add_argument('--height', type=int, default=900)
    a = p.parse_args()
    official = a.mode in ('daily-smoke', 'benchmark')
    if official and not a.aprond:
        p.error('Official-server modes require --aprond')
    port = 18766 if official else 18765
    with socket.socket() as probe:
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            probe.bind(('127.0.0.1', port))
        except OSError:
            p.error(f'Port {port} occupied; refusing to use an existing server')
    root = Path(__file__).resolve().parent.parent
    output = a.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = output / 'report.json'
    binary_digest = hashlib.sha256(a.binary.resolve().read_bytes()).hexdigest()
    children, logs = [], []
    with tempfile.TemporaryDirectory(prefix='apron-native-check-') as tmp:
        os.chmod(tmp, 0o700)
        env = os.environ.copy()
        env.update(XDG_RUNTIME_DIR=tmp, WAYLAND_DISPLAY='apron-check', XDG_STATE_HOME=tmp+'/state',
                   XDG_CACHE_HOME=tmp+'/cache', APRON_TEST_SEAT_INPUT='1', APRON_VERIFY_CLIPBOARD='1')

        def spawn(command, name):
            log = open(output / (name+'.log'), 'w')
            logs.append(log)
            child = subprocess.Popen(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
            children.append(child)
            return child

        try:
            if official:
                spawn([str(a.aprond.resolve()), '--store', 'memory', '--addr', f'127.0.0.1:{port}',
                       '--messages-per-minute', '10000', '--public-url', f'http://127.0.0.1:{port}',
                       '--upload.dir', tmp+'/uploads'], 'server')
            else:
                spawn([sys.executable, str(root/'scripts/demo_server.py')], 'server')
            deadline = time.monotonic()+10
            while True:
                if children[0].poll() is not None:
                    raise RuntimeError('Owned test server exited')
                try:
                    with socket.create_connection(('127.0.0.1', port), timeout=.2):
                        break
                except OSError:
                    if time.monotonic() > deadline:
                        raise RuntimeError('Test server startup timed out')
                    time.sleep(.05)
            arguments = ['--'+a.mode, str(report)]
            if a.mode == 'benchmark':
                seeded = subprocess.check_output([sys.executable, str(root/'scripts/seed_reference.py')], cwd=root, env=env, text=True, timeout=30)
                (output/'seed.json').write_text(seeded)
                arguments = ['--benchmark', json.loads(seeded)['room_id'], str(report)]
            compositor = spawn([a.weston, '--backend=headless', '--renderer=pixman', '--debug',
                                '--width='+str(a.width), '--height='+str(a.height), '--socket=apron-check',
                                '--shell='+a.shell, '--modules='+str(a.seat.resolve()), '--idle-time=0', '--no-config'], 'weston')
            deadline = time.monotonic()+10
            while not Path(tmp+'/apron-check').exists():
                if compositor.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('Isolated Weston startup failed')
                time.sleep(.05)
            if a.mode in ('polish-smoke', 'daily-smoke'):
                spawn([sys.executable, str(root/'scripts/capture-checkpoints.py'), str(report), '--', a.screenshooter], 'capture')
            if official:
                arguments += ['--server', 'ws://127.0.0.1:18766/ws']
            app = spawn(['dbus-run-session', '--', str(a.binary.resolve()), *arguments], 'native')
            code = app.wait(timeout=120)
            if code or not report.exists():
                raise RuntimeError(f'Native check failed ({code}); inspect {output}/native.log')
            (output/'run.json').write_text(json.dumps({'mode': a.mode, 'viewport': [a.width, a.height],
                'binary_sha256': binary_digest,
                'synthetic_owned_server': True, 'native_exit': code}, indent=2)+'\n')
            print(report)
        finally:
            for child in reversed(children):
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait()
            for log in logs:
                log.close()


if __name__ == '__main__':
    main()
