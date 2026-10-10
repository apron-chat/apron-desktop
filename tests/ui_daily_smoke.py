"""Exercise native dialogs against the official loopback Go reference server."""
import argparse
import sys
import time
from gi.repository import GLib
from apron_native.app import App, Gtk

app=App(argparse.Namespace(demo=False,server='ws://127.0.0.1:18766/ws',screenshot=None))
app.register(None);app.activate();win=app.get_active_window();ctx=GLib.MainContext.default()
errors=[]
original_hook=sys.excepthook
sys.excepthook=lambda *args: (errors.append(args), original_hook(*args))


def wait(predicate, timeout=8):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        while ctx.pending():ctx.iteration(False)
        if predicate() and not win.rendering:return
        time.sleep(.005)
    raise AssertionError('Native daily-flow timeout: '+win.session.error)


def widgets(root, cls):
    out=[]
    if isinstance(root,cls):out.append(root)
    child=root.get_first_child()
    while child:
        out.extend(widgets(child,cls));child=child.get_next_sibling()
    return out


def click(root, text):
    b=next(b for b in widgets(root,Gtk.Button) if b.get_label()==text)
    b.emit('clicked')
    return b


wait(lambda:win.session.ready and win.room_id=='general' and not win.loading)
# Create a native room dialog. Repeated activation while in flight must do one request.
d=win.room_editor()
widgets(d,Gtk.Entry)[0].set_text('Native daily synthetic room')
create=click(d,'Create');create.emit('clicked')
wait(lambda:not d.get_visible() and not win.loading)
rid=win.room_id
assert win.session.store.rooms[rid]['title']=='Native daily synthetic room'
win.editor.get_buffer().set_text('A synthetic editable message')
win.send();wait(lambda:not win.sending)
msg=win.visible_messages()[-1]
d=win.edit_message(msg)
widgets(d,Gtk.TextView)[0].get_buffer().set_text('Edited through a native dialog')
save=click(d,'Save');save.emit('clicked')
wait(lambda:not d.get_visible())
assert win.session.store.messages[msg['message_id']]['body']['text']=='Edited through a native dialog'
# A dialog opened before a newer snapshot arrives must refuse to overwrite it.
current=win.session.store.messages[msg['message_id']]
d=win.edit_message(current)
updated={**current,'log_id':str(int(current['log_id'])+1),'body':{'text':'Synthetic intervening edit'}}
win.session.store.message(updated)
click(d,'Save')
assert any('changed' in l.get_text() for l in widgets(d,Gtk.Label))
d.close()
# Keyboard entrypoint, profile, room members, preferences, search and lazy message menu.
win.search_messages()
d=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Find loaded messages')
widgets(d,Gtk.SearchEntry)[0].set_text('intervening')
wait(lambda:any('Synthetic intervening' in (b.get_label() or '') for b in widgets(d,Gtk.Button)))
d.close()
win.profile_dialog();d=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Your profile')
widgets(d,Gtk.Entry)[0].set_text('Native profile test');click(d,'Save')
wait(lambda:not d.get_visible());assert win.session.store.you['name']=='Native profile test'
win.room_details();d=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Native daily synthetic room');d.close()
# A reconnect while an editor is open invalidates that editor's operation context.
d=win.edit_message(win.session.store.messages[msg['message_id']])
generation=win.session.generation
win.session.ws.close(1000,'Synthetic interrupted edit')
wait(lambda:win.session.generation>generation and win.session.ready and not win.loading)
click(d,'Save')
assert any('Connection changed' in l.get_text() for l in widgets(d,Gtk.Label))
d.close()
win.preferences_dialog();d=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Preferences');click(d,'Cancel')
menu=win.message_menu(current);win.messages.append(menu);menu.popup()
wait(lambda:menu.get_popover() is not None)
click(menu.get_popover(),'Copy message ID')
clipboard=[]
win.get_display().get_clipboard().read_text_async(None,lambda cb,result:clipboard.append(cb.read_text_finish(result)))
wait(lambda:bool(clipboard));assert clipboard[0]==msg['message_id']
assert not errors,errors
settle=time.monotonic()+.3
wait(lambda:time.monotonic()>=settle)
path=sys.argv[1] if len(sys.argv)>1 else '/tmp/apron-daily.png'
win.capture(path)
from pathlib import Path
assert Path(path).is_file()
print('PASS: official-server native dialogs; repeated create/save; stale-edit refusal; interrupted-edit refusal; profile; search; room details; preferences; lazy menu; clipboard')
win.close()
