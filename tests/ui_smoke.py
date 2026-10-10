"""Drive actual GTK widgets on Wayland with local-only fixture content."""
import argparse
import sys
import time
from pathlib import Path
from gi.repository import GLib
from apron_gtk.app import App

options = argparse.Namespace(demo=True, screenshot=None)
app = App(options)
app.register(None)
app.activate()
win = app.get_active_window()
ctx = GLib.MainContext.default()
errors = []
original_hook = sys.excepthook
sys.excepthook = lambda *args: (errors.append(args), original_hook(*args))


def wait(predicate, seconds=8):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while ctx.pending(): ctx.iteration(False)
        if predicate() and not win.rendering:
            return
        time.sleep(.01)
    raise AssertionError('GTK smoke timeout: ' + win.session.state)


def settle():
    end = time.monotonic() + .4
    wait(lambda: time.monotonic() >= end)


wait(lambda: win.session.ready and not win.loading and bool(win.visible_messages()))
settle()
assert win.get_display().get_name().startswith('apron-wayland')
assert win.cursor is not None
assert len(win.visible_messages()) <= 50
win.load_older()
wait(lambda: not win.loading)
assert win.older and len(win.visible_messages()) <= 50
win.select_room('native')
wait(lambda: not win.loading)
assert 'thread' in win.visible_messages()[0]['body']['text'].lower()
win.editor.get_buffer().set_text('Unsent thread draft')
win.select_room('general')
wait(lambda: not win.loading)
win.select_room('native')
wait(lambda: not win.loading)
assert win.text() == 'Unsent thread draft'
win.select_room('general')
wait(lambda: not win.loading)
win.editor.get_buffer().set_text('/test-denied')
win.send()
wait(lambda: not win.sending)
assert win.text() == '/test-denied'
assert 'permission' in win.session.error.lower()
win.editor.get_buffer().set_text('A reply from the native GTK test.')
win.set_reply(win.visible_messages()[-1])
win.send()
wait(lambda: not win.sending)
assert win.text() == '' and win.reply is None
assert any(m['body']['text'] == 'A reply from the native GTK test.' and m.get('reply_to') for m in win.visible_messages())
win.editor.get_buffer().set_text('What should we build next?')
win.editor.grab_focus()
settle()
path = sys.argv[1] if len(sys.argv) > 1 else 'artifacts/preview.png'
win.capture(path)
assert Path(path).stat().st_size > 1000
assert not errors, errors
print('PASS: native Wayland UI, paging, threads, drafts, denied send, confirmed reply, screenshot', flush=True)
win.close()
