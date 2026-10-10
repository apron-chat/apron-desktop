"""Real GTK lifecycle/scroll/state regression, fictional loopback fixture only."""
import argparse
import sys
import time
import tempfile
import os
from gi.repository import GLib
from apron_gtk.app import App, Window, Gtk
from apron_gtk.model import MAX_CACHE_BYTES

state = tempfile.TemporaryDirectory(prefix='apron-state-smoke-')
os.environ['APRON_GTK_STATE_DIR'] = state.name
options=argparse.Namespace(demo=True,server=None,screenshot=None)
app=App(options);app.register(None);app.activate();win=app.get_active_window()
ctx=GLib.MainContext.default();errors=[]
original=sys.excepthook
sys.excepthook=lambda *args:(errors.append(args),original(*args))

def wait(predicate,seconds=12):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        while ctx.pending():ctx.iteration(False)
        if predicate() and not win.rendering and not win.render_pending:return
        time.sleep(.002)
    raise AssertionError('Recovery UI timeout: '+win.session.error)

def pause():
    end=time.monotonic()+.1;wait(lambda:time.monotonic()>=end)

wait(lambda:win.session.ready and not win.loading)
# Scroll anchor survives incoming edits elsewhere and unchanged rows retain identity.
adj=win.scroll.get_vadjustment();adj.set_value((adj.get_upper()-adj.get_page_size())*.4);pause()
anchor=None
for mid,(_,row) in win.row_cache.items():
    ok,bounds=row.compute_bounds(win.scroll)
    if ok and bounds.get_y()>=0:anchor=(mid,row,bounds.get_y());break
assert anchor
last=win.visible_messages()[-1]
changed={**last,'log_id':str(int(last['log_id'])+5),'body':{'text':'Synthetic changed row'}}
win.session.store.message(changed);win.queue_render();pause()
assert win.row_cache[anchor[0]][1] is anchor[1]
ok,bounds=anchor[1].compute_bounds(win.scroll)
assert abs(bounds.get_y()-anchor[2])<3,(bounds.get_y(),anchor[2])
# Room switches invalidate pending transport scans and unfinished rendering batches.
win.load_older();win.select_room('native');win.select_room('general');win.select_room('native')
wait(lambda:not win.loading)
assert win.room_id=='native'
assert all(row.apron_message_id in {m['message_id'] for m in win.visible_messages()} for _,row in win.row_cache.values())
assert len(win.row_cache)<=50 and win.session.store.cache_bytes<=MAX_CACHE_BYTES
# User scrolling during an incremental fill cancels automatic bottom following.
win.select_room('general')
end=time.monotonic()+5
while not win.rendering and time.monotonic()<end:
    ctx.iteration(False);time.sleep(.002)
win.stop_following()
win.scroll.get_vadjustment().set_value(0)
wait(lambda:not win.loading)
assert win.scroll.get_vadjustment().get_value()<3
win.select_room('native');wait(lambda:not win.loading)
# Explicitly opt into local state, preserving another room's read marker.
win.preferences.save(local_state=True)
win.session.store.reads['general']=1791536400001
win.editor.get_buffer().set_text('Synthetic durable draft')
win.reply='1791536400002';win.mention_ids=['noor']
win.persist_state()
# Interruption retains the current editor and reloads only the selected room.
gen=win.session.generation
win.session.ws.close(1000,'Synthetic interruption')
wait(lambda:win.session.generation>gen and win.session.ready and not win.loading)
assert win.text()=='Synthetic durable draft'
win.close();while_deadline=time.monotonic()+.1
while time.monotonic()<while_deadline:
    while ctx.pending():ctx.iteration(False)
    time.sleep(.002)
# A fresh window reads disk after the old writer has flushed on close.
win=Window(app,options)
wait(lambda:win.session.ready and not win.loading)
assert 'native' in win.drafts,win.drafts
win.select_room('native');wait(lambda:not win.loading)
assert win.text()=='Synthetic durable draft' and win.reply=='1791536400002'
assert win.mention_ids==['noor']
assert win.local_state.path.stat().st_size<=1024*1024
# Storage reset clears both disk and memory and prevents a close from rewriting it.
def widgets(root, cls):
    result=[root] if isinstance(root,cls) else []
    child=root.get_first_child()
    while child:result+=widgets(child,cls);child=child.get_next_sibling()
    return result
win.preferences_dialog()
prefs=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Preferences')
next(b for b in widgets(prefs,Gtk.Button) if b.get_label()=='Reset local drafts and read markers…').emit('clicked')
confirm=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Reset local drafts and read markers')
next(b for b in widgets(confirm,Gtk.Button) if b.get_label()=='Reset local data').emit('clicked')
assert not errors,errors
prefs.close();win.close()
assert not win.local_state.path.exists()
assert not win.drafts and not win.preferences.data.get('local_state')
assert not errors,errors
print('PASS: scroll anchor/row reuse, rapid room switching, bounded rows/cache, interrupted reconnect, durable draft/reply/mention restore, reset without rewrite')
state.cleanup()
