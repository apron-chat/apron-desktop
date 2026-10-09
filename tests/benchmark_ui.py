"""Small repeatable GTK/Wayland measurement, not a production benchmark."""
import argparse
import json
import os
import resource
import time
from gi.repository import GLib
from apron_native.app import App

room = os.environ['APRON_BENCH_ROOM']
app = App(argparse.Namespace(demo=False, server='ws://127.0.0.1:18766/ws', screenshot=None))
app.register(None)
start = time.monotonic()
app.activate()
win = app.get_active_window()
ctx = GLib.MainContext.default()
intervals = []
last = time.monotonic()


def beat():
    global last
    now = time.monotonic(); intervals.append(now - last); last = now
    return True


def wait(predicate, timeout=15):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        while ctx.pending(): ctx.iteration(False)
        if predicate(): return
        time.sleep(.001)
    raise AssertionError('Benchmark timeout: ' + win.session.error)


def pause(seconds):
    end = time.monotonic() + seconds
    wait(lambda: time.monotonic() >= end, seconds + 2)


def rss():
    for line in open('/proc/self/status'):
        if line.startswith('VmRSS:'): return int(line.split()[1]) / 1024


wait(lambda: win.session.ready and win.room_id == 'general' and not win.loading and bool(win.visible_messages()))
startup_ms = (time.monotonic() - start) * 1000
beat_timer = GLib.timeout_add(10, beat)
t0 = time.monotonic()
win.select_room(room)
wait(lambda: not win.loading and bool(win.visible_messages()))
pause(.1)
first_page_ms = (time.monotonic() - t0) * 1000 - 100
page_times = []
for _ in range(10):
    t0 = time.monotonic()
    win.load_older()
    wait(lambda: not win.loading)
    pause(.05)
    page_times.append((time.monotonic() - t0) * 1000 - 50)
max_heartbeat_ms = max(intervals) * 1000
# Idle measurement uses GLib's blocking main loop, not this test's polling helper.
pause(.2)
GLib.source_remove(beat_timer)
idle_loop = GLib.MainLoop()
wall0, cpu0 = time.monotonic(), time.process_time()
GLib.timeout_add_seconds(10, lambda: (idle_loop.quit(), False)[1])
idle_loop.run()
wall, cpu = time.monotonic() - wall0, time.process_time() - cpu0
result = {
 'display': win.get_display().get_name(), 'startup_to_first_history_ms': round(startup_ms, 1),
 'seed_messages': 1000, 'seed_text_bytes_each': 1045,
 'benchmark_room_latest_page_ms': round(first_page_ms, 1),
 'older_page_ms': [round(t, 1) for t in page_times],
 'max_10ms_heartbeat_interval_during_load_ms': round(max_heartbeat_ms, 1),
 'idle_seconds': round(wall, 3), 'idle_cpu_seconds': round(cpu, 4),
 'idle_percent_one_core': round(cpu / wall * 100, 3),
 'rss_mib': round(rss(), 1), 'peak_rss_mib': round(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024, 1),
 'cached_messages': len(win.session.store.messages), 'cached_message_bytes': win.session.store.cache_bytes,
 'rendered_messages': len(win.visible_messages())
}
from pathlib import Path
output = Path(__file__).resolve().parents[1] / 'artifacts' / 'benchmark.json'
output.parent.mkdir(exist_ok=True)
output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2), flush=True)
win.close()
