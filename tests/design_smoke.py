"""Visual and disclosure regression checks on real GTK widgets, synthetic fixture only."""
import argparse
from pathlib import Path
import sys
import time
from gi.repository import GLib
from apron_gtk.app import App, Gtk, Gdk

output=Path(sys.argv[1]);output.mkdir(parents=True,exist_ok=True)
app=App(argparse.Namespace(demo=True,server=None,screenshot=None));app.register(None);app.activate()
win=app.get_active_window();ctx=GLib.MainContext.default();errors=[]
original=sys.excepthook
sys.excepthook=lambda *args:(errors.append(args),original(*args))


def wait(predicate,seconds=8):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        while ctx.pending():ctx.iteration(False)
        if predicate() and not win.rendering:return
        time.sleep(.005)
    raise AssertionError('Design smoke timeout: '+win.session.error)


def settle():
    end=time.monotonic()+.25;wait(lambda:time.monotonic()>=end)


def widgets(root,cls):
    out=[root] if isinstance(root,cls) else []
    child=root.get_first_child()
    while child:out+=widgets(child,cls);child=child.get_next_sibling()
    return out


def capture(widget,name):
    node=None
    for _ in range(8):
        widget.queue_draw(); settle()
        snapshot=Gtk.Snapshot();Gtk.WidgetPaintable.new(widget).snapshot(snapshot,widget.get_width(),widget.get_height())
        node=snapshot.to_node()
        if node is not None: break
    assert node is not None,(name,widget.get_mapped(),widget.get_width(),widget.get_height())
    widget.get_renderer().render_texture(node,None).save_to_png(str(output/name))
    assert (output/name).stat().st_size>1000


def account_profile_enabled():
    was_shown=win.split.get_show_sidebar()
    if win.split.get_collapsed(): win.split.set_show_sidebar(True); settle()
    win.account_menu.popup();wait(lambda:win.account_menu.get_popover() is not None)
    pop=win.account_menu.get_popover()
    enabled=next(b for b in widgets(pop,Gtk.Button) if b.get_label()=='Your profile…').get_sensitive()
    pop.popdown();settle()
    if win.split.get_collapsed(): win.split.set_show_sidebar(was_shown); settle()
    return enabled


wait(lambda:win.session.ready and win.room_id=='general' and not win.loading and bool(win.visible_messages()))
win.editor.grab_focus();settle()
assert not win.latest_button.get_visible()
assert win.composer_placeholder.get_visible()
capture(win,'light.png')
# No persistent sidebar-button stack or global header toolbar remains.
assert not hasattr(win,'profile_button') and not hasattr(win,'thread_button')
assert account_profile_enabled()
assert account_profile_enabled()  # reopened lazy menu must rebuild safely
win.room_menu.popup();wait(lambda:win.room_menu.get_popover() is not None)
assert any(b.get_label()=='Start a thread…' for b in widgets(win.room_menu.get_popover(),Gtk.Button))
win.room_menu.get_popover().popdown();settle()
# Focus and keyboard context access reveal actions without mouse hover.
row=win.messages.get_last_child();assert row.grab_focus();settle()
assert row.get_state_flags() & Gtk.StateFlags.FOCUS_WITHIN, (win.get_focus(), row.get_root(), row.get_state_flags())
capture(win,'message-focus.png')
# An incoming snapshot must not tear down an open contextual menu.
menu=next(b for b in widgets(row,Gtk.MenuButton))
menu.popup();wait(lambda:win.message_menu_open)
pop=menu.get_popover();last=win.visible_messages()[-1]
updated={**last,'log_id':str(int(last['log_id'])+1),'body':{**last.get('body',{}),'text':'Synthetic update while menu is open'}}
win.session.store.message(updated);win.queue_render();settle()
assert menu.get_popover() is pop and win.message_menu_open
pop.popdown();wait(lambda:not win.message_menu_open);settle()
win.editor.grab_focus()
assert win.key_pressed(None,Gdk.KEY_Page_Up,0,Gdk.ModifierType.CONTROL_MASK)
wait(lambda:not win.loading);assert win.older and win.latest_button.get_visible()
assert win.key_pressed(None,Gdk.KEY_Page_Down,0,Gdk.ModifierType.CONTROL_MASK)
wait(lambda:not win.loading);settle();assert not win.older
# Empty and loading states use the same calm conversation surface.
win.select_room('weekend');wait(lambda:not win.loading);capture(win,'empty.png')
win.loading=True;win.queue_render();capture(win,'loading.png')
win.loading=False;win.select_room('general');wait(lambda:not win.loading)
# All forms use the same native header, content spacing and trailing action row.
win.profile_dialog();dialog=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Your profile')
capture(dialog,'profile.png');dialog.close();settle()
win.preferences_dialog();dialog=next(w for w in Gtk.Window.get_toplevels() if w.get_title()=='Preferences')
capture(dialog,'preferences.png')
next(b for b in widgets(dialog,Gtk.Button) if b.get_label()=='Cancel').emit('clicked');settle()
win.preferences.data['theme']='dark';win.apply_theme();win.editor.grab_focus();capture(win,'dark.png')
win.preferences.data['theme']='light';win.apply_theme();settle()
# Keep safety feedback and drafts through interrupted connection and menu reopening.
win.editor.get_buffer().set_text('Synthetic unsent draft')
win.session.disconnect();settle()
assert win.banner.get_revealed() and not win.send_button.get_sensitive()
assert win.text()=='Synthetic unsent draft' and not account_profile_enabled()
capture(win,'disconnected.png')
win.session.connect('ws://127.0.0.1:18765','You')
wait(lambda:win.session.ready and not win.loading);settle();assert account_profile_enabled()
assert not errors,errors
print('PASS: design light/dark, desktop/compact layout, focused message actions, repeated contextual menus, history shortcuts, profile/preferences, disconnect safety and drafts')
win.close()
