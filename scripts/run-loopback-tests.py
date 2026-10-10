#!/usr/bin/env python3
"""Run only synthetic tests using owned child servers; requires Python websockets."""
import argparse
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--aprond', required=True, help='Path to a separately built official Apron server')
    args = parser.parse_args()
    server = shutil.which(args.aprond)
    if server is None:
        parser.error('Apron server executable not found')
    root = pathlib.Path(__file__).resolve().parent.parent
    # Never connect the integration suite to a pre-existing (possibly real) server.
    for port in (18765, 18766):
        with socket.socket() as probe:
            try:
                probe.bind(('127.0.0.1', port))
            except OSError:
                parser.error(f'Port {port} is occupied; refusing to use an existing server')
    children = []
    with tempfile.TemporaryDirectory(prefix='apron-gpui-tests-') as tmp:
        logs = []
        try:
            commands = [
                [server, '--store', 'memory', '--addr', '127.0.0.1:18766',
                 '--messages-per-minute', '10000', '--public-url', 'http://127.0.0.1:18766',
                 '--upload.dir', str(pathlib.Path(tmp) / 'uploads')],
                [sys.executable, str(root / 'scripts/demo_server.py')],
            ]
            for index, command in enumerate(commands):
                log = open(pathlib.Path(tmp) / f'server-{index}.log', 'w+')
                logs.append(log)
                children.append(subprocess.Popen(command, cwd=root, stdout=log, stderr=log))
            deadline = time.monotonic() + 10
            for port in (18765, 18766):
                while True:
                    if any(p.poll() is not None for p in children):
                        raise RuntimeError('A test server exited before becoming ready')
                    try:
                        with socket.create_connection(('127.0.0.1', port), timeout=.2):
                            break
                    except OSError:
                        if time.monotonic() >= deadline:
                            raise RuntimeError('Test server startup timed out')
                        time.sleep(.05)
            subprocess.run(['cargo', 'test', '--locked', '--no-default-features',
                            '--test', 'loopback', '--', '--ignored', '--test-threads=1'],
                           cwd=root, check=True)
        except (RuntimeError, subprocess.CalledProcessError):
            for log in logs:
                log.flush()
                log.seek(0)
                print(log.read(), file=sys.stderr)
            raise
        finally:
            for child in children:
                if child.poll() is None:
                    child.terminate()
            for child in children:
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
            for log in logs:
                log.close()


if __name__ == '__main__':
    main()
