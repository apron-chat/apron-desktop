"""Real GTK controls plus official loopback server; deterministic local chooser results."""
import argparse
import tempfile
from pathlib import Path
import sys
import time
from unittest.mock import patch
from gi.repository import GLib,Gio
from apron_gtk.app import App,Gtk
from apron_gtk.session import Session

app=App(argparse.Namespace(demo=False,server='ws://127.0.0.1:18766/ws',screenshot=None));app.register(None);app.activate()
win=app.get_active_window();ctx=GLib.MainContext.default();errors=[];old_hook=sys.excepthook
sys.excepthook=lambda *args:(errors.append(args),old_hook(*args))
def wait(predicate,seconds=10):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        while ctx.pending():ctx.iteration(False)
        if predicate() and not win.rendering:return
        time.sleep(.002)
    raise AssertionError('Native presence/media timeout: '+win.session.error+' '+win.transfer_note.get_text())
def widgets(root,cls):
    result=[root] if isinstance(root,cls) else []
    child=root.get_first_child()
    while child:result+=widgets(child,cls);child=child.get_next_sibling()
    return result
def click(root,text):
    b=next(b for b in widgets(root,Gtk.Button) if b.get_label()==text);b.emit('clicked');return b
wait(lambda:win.session.ready and not win.loading)
assert win.get_application().get_application_id()=='chat.apron.Gtk'
assert win.attach_button.get_visible()
other=Session();other.connect('ws://127.0.0.1:18766/ws','Synthetic typing peer')
wait(lambda:other.ready and 'general' in other.store.rooms)
other.notify('activity',{'room_id':'general','typing':15})
wait(lambda:win.typing_label.get_visible())
assert 'typing' in win.typing_label.get_text()
other.notify('activity',{'room_id':'general','typing':0});wait(lambda:not win.typing_label.get_visible())
# Actual composer user-action signal drives throttled typing, not restored draft text.
b=win.editor.get_buffer();b.set_text('');b.begin_user_action();b.insert_at_cursor('Synthetic unsent composer');b.end_user_action()
wait(lambda:('general',win.session.store.you['user_id']) in other.presence.typing)
win.select_room('general');wait(lambda:not win.loading)
wait(lambda:('general',win.session.store.you['user_id']) not in other.presence.typing)
d=win.mute_dialog('general');widgets(d,Gtk.DropDown)[0].set_selected(3);click(d,'Apply')
wait(lambda:not d.get_visible() and 'general' in win.session.presence.mutes)
win.preferences.data['notifications']=True
with patch.object(win,'is_active',return_value=False),patch.object(app,'send_notification') as notify:
    win.incoming({'message_id':'10','log_id':'10','room_id':'general','body':{'mentions':[win.session.store.you['user_id']]}})
    notify.assert_not_called()
win.preferences.data['notifications']=False
# File chooser callbacks supply only synthetic local paths; the confirmation/widgets,
# Gio reads/writes, WebSocket post, HTTP PUT/GET and final server snapshot are real.
with tempfile.TemporaryDirectory(prefix='apron-gtk-media-') as directory:
    source=Path(directory)/'synthetic.txt';source.write_bytes(b'Synthetic native attachment')
    second=Path(directory)/'second.txt';second.write_bytes(b'Synthetic second attachment')
    target=Path(directory)/'saved.txt'
    class Picker:
        def __init__(self,**kwargs):pass
        def open_multiple(self,parent,cancel,callback):GLib.idle_add(lambda:(callback(self,None),False)[1])
        def open_multiple_finish(self,result):
            files=Gio.ListStore.new(Gio.File);files.append(Gio.File.new_for_path(str(source)));files.append(Gio.File.new_for_path(str(second)));return files
        def save(self,parent,cancel,callback):GLib.idle_add(lambda:(callback(self,None),False)[1])
        def save_finish(self,result):return Gio.File.new_for_path(str(target))
    with patch.object(Gtk,'FileDialog',Picker):
        # Seed a real reply target, then carry the current composer into a batch.
        posted=[];win.session.send('general','Synthetic reply target',callback=lambda *a:posted.append(a));wait(lambda:bool(posted))
        reply_id=posted[0][0]['message_id'];win.set_reply(win.session.store.messages[reply_id])
        win.choose_attachment();wait(lambda:win.attachment_dialog is not None)
        d=win.attachment_dialog
        assert widgets(d,Gtk.CheckButton)[0].get_active()
        if len(sys.argv)>2:
            settle=time.monotonic()+.25;wait(lambda:time.monotonic()>=settle)
            type(win).capture(d,sys.argv[2])
        send=click(d,'Send attachments');send.emit('clicked')
        wait(lambda:not win.transfers.active and 'uploaded' in win.transfer_note.get_text())
        def uploads():return [m for m in win.session.store.timeline('general') if m.get('reply_to',{}).get('message_id')==reply_id and len(m.get('body',{}).get('embeds',[]))==2 and all(e.get('url') for e in m['body']['embeds'])]
        wait(lambda:bool(uploads()))
        message=uploads()[-1]
        assert len(message['body']['embeds'])==2
        assert message['reply_to']['message_id']==reply_id
        assert message['body']['text']=='Synthetic unsent composer'
        assert win.text()=='' and win.reply is None
        embed=message['body']['embeds'][0]
        win.download_attachment(embed);wait(lambda:target.exists() and win.transfer_note.get_text()=='Attachment saved.')
        assert target.read_bytes()==source.read_bytes()
        assert win.text()==''
    # A failed second PUT stops the batch; already-posted text is not resent,
    # and composer edits made after opening the confirmation survive acceptance.
    win.editor.get_buffer().set_text('Synthetic batch caption')
    d=win.confirm_attachments([('one.txt',b'one'),('two.txt',b'two'),('three.txt',b'three')],'general',win.session.generation)
    win.editor.get_buffer().set_text('Newer unsent draft')
    calls=[];real_request=win.transfers.request
    def fail_second(method,url,server,callback,data=None):
        calls.append(data)
        if len(calls)==2:
            GLib.idle_add(lambda:(callback(None,'Synthetic second-file failure.'),False)[1]);return
        return real_request(method,url,server,callback,data)
    with patch.object(win.transfers,'request',side_effect=fail_second):
        click(d,'Send attachments')
        wait(lambda:'Synthetic second-file failure.' in win.transfer_note.get_text())
    assert calls==[b'one',b'two'],calls
    assert '1 of 3 uploaded' in win.transfer_note.get_text()
    assert win.text()=='Newer unsent draft'
    settle=time.monotonic()+.3;wait(lambda:time.monotonic()>=settle)
    if len(sys.argv)>1:win.capture(sys.argv[1])
    # Opening stale upload confirmation must not post after the connection changes.
    d=win.confirm_attachment('stale.txt',b'Synthetic','general',win.session.generation)
    win.session.disconnect();click(d,'Send attachments')
    assert any('Connection changed' in l.get_text() for l in widgets(d,Gtk.Label))
    d.close();win.attachment_dialog=None
assert not errors,errors
other.disconnect();win.close()
print('PASS: apron-gtk identity, typing row and user-action signal, room mute and notification suppression, confirmed two-file reply, duplicate guard, explicit download/save, composer clear after acceptance, changed-draft preservation, partial failure stops batch, stale attachment refusal')
