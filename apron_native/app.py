import argparse
from datetime import datetime
from pathlib import Path
import sys
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, Gio, GLib, Gdk, Pango
from .model import PAGE_SIZE, safe_text, number
from .session import Session
from .render import markdown, embed_text
from .preferences import Preferences
from .keyring import Keyring
from .ui_actions import DailyActions


def box(vertical=False, spacing=8, margin=0):
    b = Gtk.Box(orientation=Gtk.Orientation.VERTICAL if vertical else Gtk.Orientation.HORIZONTAL, spacing=spacing)
    for side in ('top', 'bottom', 'start', 'end'):
        getattr(b, 'set_margin_' + side)(margin)
    return b


def label(text='', css=None, wrap=False):
    w = Gtk.Label(label=safe_text(text), xalign=0)
    if css:
        w.add_css_class(css)
    w.set_wrap(wrap)
    w.set_wrap_mode(Pango.WrapMode.WORD_CHAR)
    return w


def button(text, callback, css=None):
    w = Gtk.Button(label=text)
    w.connect('clicked', lambda *_: callback())
    if css:
        w.add_css_class(css)
    return w


def clear(container):
    while child := container.get_first_child():
        container.remove(child)


class Window(DailyActions, Adw.ApplicationWindow):
    def __init__(self, app, options):
        super().__init__(application=app, title='Apron', default_width=1120, default_height=800)
        self.options = options
        self.preferences = Preferences()
        self.keyring = Keyring()
        self.apply_theme()
        self.reaction_pending = set()
        self.keyring_jobs = {}
        self.keyring_busy = False
        self.session = Session(self.queue_render)
        self.session.on_ready = self.ready
        self.session.on_token = self.save_secure_session
        self.session.on_message = self.incoming
        self.room_id = ''
        self.cursor = None
        self.page_ids = None
        self.older = False
        self.loading = False
        self.load_generation = 0
        self.drafts = {}
        self.reply = None
        self.mention_ids = []
        self.sending = False
        self.render_pending = False
        self.list_signature = self.message_signature = None
        self.focus_bottom = True
        self.build()
        self.connect('close-request', self.close_requested)
        self.connect('notify::is-active', lambda *_: self.read_visible())
        breakpoint = Adw.Breakpoint.new(Adw.BreakpointCondition.parse('max-width: 760px'))
        breakpoint.add_setter(self.split, 'collapsed', True)
        breakpoint.add_setter(self.thread_button, 'visible', False)
        self.add_breakpoint(breakpoint)
        self.split.bind_property('collapsed', self.nav, 'visible', 2)
        self.present()
        if getattr(options, 'server', None):
            self.session.connect(options.server, 'You')
        elif options.demo:
            self.session.connect('ws://127.0.0.1:18765', 'You')
        else:
            GLib.idle_add(self.connect_dialog)
        if options.screenshot:
            GLib.timeout_add_seconds(4, self.capture, options.screenshot)

    def build(self):
        self.split = Adw.OverlaySplitView()
        self.split.set_min_sidebar_width(220)
        self.split.set_max_sidebar_width(280)
        self.split.set_sidebar_width_fraction(0.24)
        sidebar = box(True, 16)
        sidebar.add_css_class('sidebar')
        brand = box(margin=20)
        brand.append(label('a', 'brand-icon'))
        names = box(True, 2)
        names.append(label('apron', 'brand'))
        names.append(label('A little closer.', 'dim-label'))
        brand.append(names)
        sidebar.append(brand)
        self.search = Gtk.SearchEntry(placeholder_text='Find a room  ·  Ctrl+K')
        self.search.set_margin_start(16); self.search.set_margin_end(16)
        self.search.connect('search-changed', lambda *_: self.render_rooms(force=True))
        sidebar.append(self.search)
        section = label('YOUR SPACES', 'section-label')
        section.set_margin_start(24)
        sidebar.append(section)
        self.rooms = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        self.rooms.add_css_class('room-list')
        self.rooms.connect('row-activated', self.activate_room)
        scroll = Gtk.ScrolledWindow(vexpand=True, hscrollbar_policy=Gtk.PolicyType.NEVER)
        scroll.set_child(self.rooms)
        sidebar.append(scroll)
        bottom = box(True, 8, 16)
        self.identity = label('Not signed in', 'author')
        self.status = label('Disconnected', 'connection')
        bottom.append(self.identity); bottom.append(self.status)
        bottom.append(button('Connection settings', self.connect_dialog))
        self.profile_button = button('Profile', self.profile_dialog)
        bottom.append(self.profile_button)
        bottom.append(button('Preferences', self.preferences_dialog))
        self.create_room_button = button('Create a room', self.room_editor)
        bottom.append(self.create_room_button)
        sidebar.append(bottom)
        self.split.set_sidebar(sidebar)

        content = box(True, 0)
        header = Adw.HeaderBar()
        self.nav = button('☰', lambda: self.split.set_show_sidebar(not self.split.get_show_sidebar()))
        self.nav.set_visible(False)
        self.nav.set_tooltip_text('Show rooms')
        header.pack_start(self.nav)
        self.title_label = label('Welcome to Apron', 'room-title')
        self.title_label.set_ellipsize(Pango.EllipsizeMode.END)
        self.title_label.set_max_width_chars(22)
        header.set_title_widget(self.title_label)
        self.thread_button = button('New thread', self.new_thread)
        header.pack_end(self.thread_button)
        self.details_button = button('Room', self.room_details)
        self.details_button.set_tooltip_text('Room details, members, and management')
        header.pack_end(self.details_button)
        header.pack_start(button('Find', self.search_messages))
        shortcuts = button('?', self.shortcuts_dialog)
        shortcuts.set_tooltip_text('Keyboard shortcuts')
        header.pack_start(shortcuts)
        content.append(header)
        self.banner = Adw.Banner(title='Connect to your server to begin.')
        content.append(self.banner)
        tools = box(margin=16)
        self.subtitle = label('Native on Linux. At home on Wayland.', 'dim-label')
        self.subtitle.set_ellipsize(Pango.EllipsizeMode.END)
        self.subtitle.set_hexpand(True)
        tools.append(self.subtitle)
        self.join = button('Join room', self.join_room, 'suggested-action')
        tools.append(self.join)
        self.old_button = button('← Older', self.load_older)
        tools.append(self.old_button)
        self.latest_button = button('Latest ↓', lambda: self.load_history(latest=True))
        tools.append(self.latest_button)
        content.append(tools)
        self.messages = box(True, 10, 20)
        self.messages.set_valign(Gtk.Align.START)
        clamp = Adw.Clamp(maximum_size=900, tightening_threshold=700)
        clamp.set_child(self.messages)
        self.scroll = Gtk.ScrolledWindow(vexpand=True, hscrollbar_policy=Gtk.PolicyType.NEVER)
        self.scroll.set_child(clamp)
        self.scroll.get_vadjustment().connect('value-changed', self.scrolled)
        content.append(self.scroll)
        composer_outer = box(True, 8, 20)
        self.reply_box = box()
        self.reply_label = label('', 'dim-label')
        self.reply_label.set_hexpand(True); self.reply_label.set_ellipsize(Pango.EllipsizeMode.END)
        self.reply_box.append(self.reply_label)
        self.reply_box.append(button('Cancel reply', self.cancel_reply))
        composer_outer.append(self.reply_box)
        compose = box(True, 8)
        compose.add_css_class('composer')
        self.editor = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, top_margin=4, bottom_margin=4)
        self.editor.set_accepts_tab(False)
        self.editor.set_size_request(-1, 64)
        self.editor.get_buffer().connect('changed', lambda *_: self.update_send())
        editor_scroll = Gtk.ScrolledWindow(min_content_height=64, max_content_height=150, propagate_natural_height=True)
        editor_scroll.set_child(self.editor)
        compose.append(editor_scroll)
        actions = box()
        self.mention_button = button('@ Mention', self.mention_dialog)
        actions.append(self.mention_button)
        self.hint = label('Ctrl+Enter to send', 'metadata')
        self.hint.set_hexpand(True)
        actions.append(self.hint)
        self.send_button = button('Send ↑', self.send, 'suggested-action')
        actions.append(self.send_button)
        compose.append(actions)
        composer_outer.append(compose)
        content.append(composer_outer)
        self.split.set_content(content)
        self.set_content(self.split)
        keys = Gtk.EventControllerKey()
        keys.connect('key-pressed', self.key_pressed)
        self.add_controller(keys)
        self.render()

    def key_pressed(self, _, key, code, state):
        ctrl = bool(state & Gdk.ModifierType.CONTROL_MASK)
        alt = bool(state & Gdk.ModifierType.ALT_MASK)
        if ctrl and key == Gdk.KEY_f:
            self.search_messages(); return True
        if alt and key == Gdk.KEY_n:
            self.next_unread(); return True
        if alt and key == Gdk.KEY_Up:
            parent = self.session.store.rooms.get(self.room_id, {}).get('parent_room_id')
            if parent: self.select_room(parent)
            return True
        if ctrl and key == Gdk.KEY_Return:
            self.send(); return True
        if ctrl and key == Gdk.KEY_k:
            self.split.set_show_sidebar(True); self.search.grab_focus(); return True
        if ctrl and key == Gdk.KEY_l:
            self.editor.grab_focus(); return True
        if key == Gdk.KEY_Escape:
            self.cancel_reply(); return True
        return False

    def queue_render(self):
        if not self.render_pending:
            self.render_pending = True
            GLib.idle_add(self.render)

    def render(self):
        self.render_pending = False
        s = self.session
        self.status.set_text(('● ' if s.ready else '○ ') + s.state)
        self.identity.set_text(safe_text(s.store.you.get('name') or s.store.you.get('user_id') or 'Not signed in'))
        room = s.store.rooms.get(self.room_id, {})
        self.title_label.set_text(safe_text(room.get('title') or self.room_id or 'Apron'))
        self.subtitle.set_text(safe_text(room.get('description') or ('Thread' if room.get('parent_room_id') else 'Room conversation')))
        self.banner.set_title(safe_text(s.error or ('' if s.ready else s.state + ' · Messages are read-only until connected.')))
        self.banner.set_revealed(bool(s.error or not s.ready))
        self.profile_button.set_sensitive(s.ready)
        self.create_room_button.set_sensitive(s.ready and 'rooms' in s.caps)
        self.details_button.set_sensitive(s.ready and bool(self.room_id))
        self.thread_button.set_sensitive(s.ready and 'rooms' in s.caps and bool(self.room_id))
        self.join.set_visible(s.ready and 'rooms' in s.caps and bool(self.room_id) and not room.get('_joined'))
        self.old_button.set_sensitive(s.ready and self.cursor is not None and not self.loading)
        self.latest_button.set_sensitive(s.ready and 'history' in s.caps and not self.loading)
        self.old_button.set_visible('history' in s.caps)
        self.latest_button.set_visible('history' in s.caps)
        self.reply_box.set_visible(self.reply is not None)
        self.mention_button.set_sensitive(s.ready and bool(room.get('members')))
        self.render_rooms()
        self.render_messages()
        self.update_send()
        return False

    def render_rooms(self, force=False):
        s = self.session.store
        q = self.search.get_text().lower()
        rooms = sorted(s.rooms.values(), key=lambda r: (r.get('parent_room_id', r['room_id']), bool(r.get('parent_room_id')), r.get('title', r['room_id'])))
        signature = repr((rooms, s.unread, s.mentions, q, self.room_id))
        if not force and signature == self.list_signature:
            return
        self.list_signature = signature
        clear(self.rooms)
        for r in rooms:
            rid = r['room_id']; name = r.get('title') or rid
            if q and q not in (name + ' ' + rid).lower():
                continue
            row = Gtk.ListBoxRow()
            row.rid = rid
            b = box()
            b.append(label('↳' if r.get('parent_room_id') else '#', 'dim-label'))
            title = label(name)
            title.set_hexpand(True); title.set_ellipsize(Pango.EllipsizeMode.END)
            b.append(title)
            n = s.unread.get(rid, 0); mentions = s.mentions.get(rid, 0)
            if n or mentions:
                b.append(label(('@ ' if mentions else '') + str(n or mentions), 'badge'))
            elif not r.get('_joined'):
                b.append(label('+', 'dim-label'))
            row.set_child(b)
            self.rooms.append(row)
            if rid == self.room_id:
                self.rooms.select_row(row)

    def visible_messages(self):
        timeline = self.session.store.timeline(self.room_id)
        if self.older and self.page_ids is not None:
            return [m for m in timeline if m['message_id'] in self.page_ids][-PAGE_SIZE:]
        return timeline[-PAGE_SIZE:]

    def render_messages(self):
        messages = self.visible_messages()
        # A loading-state transition must not rebuild an unchanged nonempty page.
        signature = repr((messages, self.session.store.users, self.session.store.reactions, self.session.ready, self.session.caps, self.room_id, self.loading if not messages else False))
        if signature == self.message_signature:
            return
        self.message_signature = signature
        was_bottom = self.focus_bottom
        clear(self.messages)
        if not messages:
            status = Adw.StatusPage(title='Loading conversation…' if self.loading else 'A space for conversation',
                                    description='Connect to a server, choose a room, and say hello.' if not self.session.ready else 'No messages in this page yet.',
                                    icon_name='chat-message-new-symbolic')
            self.messages.append(status)
        else:
            note = label(('OLDER MESSAGES' if self.older else 'LATEST MESSAGES') + f'  ·  {len(messages)} shown', 'section-label')
            self.messages.append(note)
            for msg in messages:
                self.messages.append(self.message_widget(msg))
        if was_bottom and not self.older:
            GLib.idle_add(self.scroll_bottom)

    def message_widget(self, msg):
        outer = box(spacing=12)
        user = msg.get('from', {})
        uid = user.get('user_id', '?')
        user = {**user, **self.session.store.users.get(uid, {})}
        name = safe_text(user.get('name') or uid)
        avatar = label(name[:1].upper(), 'avatar')
        avatar.set_xalign(0.5); avatar.set_valign(Gtk.Align.START)
        outer.append(avatar)
        card = box(True, 7)
        card.set_hexpand(True); card.add_css_class('message')
        if self.session.store.you.get('user_id') in msg.get('body', {}).get('mentions', []):
            card.add_css_class('mentioned')
        top = box()
        author = label(name, 'author')
        author.set_ellipsize(Pango.EllipsizeMode.END); author.set_max_width_chars(18)
        top.append(author)
        user_label = label('@' + uid, 'metadata')
        user_label.set_hexpand(True); user_label.set_ellipsize(Pango.EllipsizeMode.END)
        top.append(user_label)
        try:
            stamp = datetime.fromtimestamp(number(msg['message_id']) / 1000).strftime('%H:%M')
        except (ValueError, OverflowError, OSError):
            stamp = ''
        top.append(label(stamp, 'metadata'))
        if not msg.get('deleted'):
            reply = button('Reply', lambda: self.set_reply(msg), 'flat')
            reply.add_css_class('reply-button')
            reply.set_tooltip_text('Reply to ' + name)
            top.append(reply)
        top.append(self.message_menu(msg))
        card.append(top)
        target = msg.get('reply_to', {})
        if target.get('message_id'):
            original = self.session.store.messages.get(target['message_id'], target)
            excerpt = original.get('body', {}).get('text') or 'Message ' + target['message_id']
            card.append(label('↳ ' + safe_text(excerpt, 100), 'dim-label', True))
        body = msg.get('body', {})
        text = label('', 'message-body', True)
        text.set_selectable(True)
        if msg.get('deleted'):
            text.set_text('Message deleted'); text.add_css_class('dim-label')
        elif body.get('format') == 'markdown':
            text.set_markup(markdown(body.get('text', '')))
        else:
            text.set_text(safe_text(body.get('text', '')))
        card.append(text)
        for embed in body.get('embeds', [])[:12]:
            card.append(label('▧ ' + embed_text(embed), 'dim-label', True))
        if not msg.get('deleted') and 'reactions' in self.session.caps:
            reactions = box()
            for emoji, count in self.session.store.reaction_counts(msg['message_id']).items():
                reactions.append(button(safe_text(emoji) + ' ' + str(count), lambda e=emoji: self.toggle_reaction(msg, e), 'flat'))
            add = button('React', lambda: self.reaction_dialog(msg), 'flat')
            add.set_sensitive(self.session.ready)
            reactions.append(add); card.append(reactions)
        outer.append(card)
        return outer

    def ready(self):
        rooms = self.session.store.rooms
        if self.room_id not in rooms:
            self.room_id = next((rid for rid, r in rooms.items() if r.get('_joined') and not r.get('parent_room_id')), next(iter(rooms), ''))
        self.load_history(latest=True)
        self.queue_render()

    def incoming(self, msg):
        if not self.room_id:
            self.room_id = msg['room_id']
        if self.preferences.data.get('notifications') and self.session.ready and not self.is_active() and msg.get('message_id') == msg.get('log_id') and self.session.store.you.get('user_id') in msg.get('body', {}).get('mentions', []):
            notice = Gio.Notification.new('New Apron mention')
            notice.set_body('Mention in ' + safe_text(self.session.store.rooms.get(msg['room_id'], {}).get('title') or msg['room_id'], 120))
            notice.set_default_action_and_target('app.open-room', GLib.Variant('s', msg['room_id']))
            self.get_application().send_notification(self.notification_id(msg['room_id']), notice)
        self.queue_render()

    @staticmethod
    def notification_id(rid):
        import hashlib
        return 'room-' + hashlib.sha256(rid.encode()).hexdigest()[:24]

    def activate_room(self, _, row):
        self.select_room(row.rid)

    def text(self):
        b = self.editor.get_buffer()
        return b.get_text(b.get_start_iter(), b.get_end_iter(), False)

    def select_room(self, rid):
        self.drafts[self.room_id] = (self.text(), self.reply, self.mention_ids[:])
        while len(self.drafts) > 256:
            self.drafts.pop(next(iter(self.drafts)))
        self.room_id = rid
        text, self.reply, self.mention_ids = self.drafts.get(rid, ('', None, []))
        self.editor.get_buffer().set_text(text)
        self.reply_label.set_text('Replying to message ' + (self.reply or ''))
        self.focus_bottom = True
        if self.split.get_collapsed():
            self.split.set_show_sidebar(False)
        self.load_history(latest=True)
        if 'rooms' in self.session.caps and self.session.ready:
            self.session.list_rooms(parent=rid)
        self.queue_render()

    def load_history(self, latest=False):
        self.load_generation += 1
        generation = self.load_generation
        if not self.session.ready or 'history' not in self.session.caps:
            self.loading = False; self.cursor = None; self.older = False
            self.queue_render(); return
        self.loading = True
        rid = self.room_id
        before = None if latest else self.cursor
        self.older = not latest
        if latest:
            self.page_ids = None
            self.focus_bottom = True
        def done(result, error, cursor):
            if generation != self.load_generation:
                return
            self.loading = False
            if not error:
                self.cursor = cursor
                if not latest:
                    self.page_ids = {m['message_id'] for m in result.get('messages', [])}
                    GLib.idle_add(lambda: self.scroll.get_vadjustment().set_value(0))
            self.queue_render()
        self.session.history(rid, before, done)
        self.queue_render()

    def load_older(self):
        if self.cursor is not None and not self.loading:
            self.load_history()

    def scrolled(self, adjustment):
        self.focus_bottom = adjustment.get_value() >= adjustment.get_upper() - adjustment.get_page_size() - 30
        self.read_visible()

    def scroll_bottom(self):
        adj = self.scroll.get_vadjustment()
        adj.set_value(max(0, adj.get_upper() - adj.get_page_size()))
        self.read_visible()
        return False

    def read_visible(self):
        if not self.is_active() or not self.focus_bottom or self.older or self.loading or not self.session.ready:
            return
        messages = self.visible_messages()
        if messages:
            mid = messages[-1]['message_id']
            store = self.session.store
            if store.reads.get(self.room_id, 0) < number(mid):
                store.mark_read(self.room_id, mid)
                if self.preferences.data.get('notifications'):
                    self.get_application().withdraw_notification(self.notification_id(self.room_id))
                if 'activity' in self.session.caps:
                    self.session.notify('activity', {'room_id': self.room_id, 'read_message_id': mid})
                self.queue_render()

    def update_send(self):
        if not hasattr(self, 'send_button'):
            return
        text = self.text()
        self.send_button.set_sensitive(self.session.ready and bool(text.strip()) and len(text.encode()) <= 32000 and not self.sending)
        self.send_button.set_label('Sending…' if self.sending else 'Send ↑')
        self.hint.set_text('Message limit: 32 KB' if len(text.encode()) > 32000 else
                           (f'{len(self.mention_ids)} mentioned · ' if self.mention_ids else '') + 'Ctrl+Enter to send')

    def send(self):
        if not self.send_button.get_sensitive():
            return
        text, rid, reply = self.text(), self.room_id, self.reply
        self.sending = True
        self.update_send()
        def done(result, error):
            self.sending = False
            if not error:
                if rid == self.room_id and self.text() == text:
                    self.editor.get_buffer().set_text('')
                    self.cancel_reply(); self.mention_ids = []
                elif self.drafts.get(rid, ('',))[0] == text:
                    self.drafts[rid] = ('', None, [])
                self.session.error = ''
            self.queue_render()
        mentions = self.mention_ids[:]
        def post():
            self.session.send(rid, text, reply, mentions, done)
        if rid and 'rooms' in self.session.caps and not self.session.store.rooms.get(rid, {}).get('_joined'):
            def joined(result, error):
                if error: done(None, error)
                else: post()
            self.session.request('room_join', {'room_id': rid}, joined)
        else:
            post()

    def set_reply(self, msg):
        self.reply = msg['message_id']
        self.reply_label.set_text('Replying to ' + safe_text(msg.get('from', {}).get('name') or msg.get('from', {}).get('user_id', ''), 100))
        self.reply_box.set_visible(True)
        self.editor.grab_focus()

    def cancel_reply(self):
        self.reply = None
        self.reply_box.set_visible(False)

    def dialog(self, title):
        win = Adw.Window(title=title, transient_for=self, modal=True, default_width=460)
        root = box(True, 16, 24)
        heading = label(title, 'title-1')
        root.append(heading)
        win.set_content(root)
        return win, root

    def connect_dialog(self):
        win, root = self.dialog('Your Apron server')
        root.append(label('Guest or token sign-in. Credentials stay in memory and are forgotten when you close the app.', 'dim-label', True))
        url = Gtk.Entry(placeholder_text='wss://chat.example/ws', text=self.session.url or self.preferences.data.get('url') or 'ws://127.0.0.1:18765')
        name = Gtk.Entry(placeholder_text='Display name', text=self.session.name or self.preferences.data.get('name') or 'You')
        token = Gtk.PasswordEntry(placeholder_text='Token (leave empty for guest)', show_peek_icon=True)
        for title, widget in [('Server address', url), ('Display name', name), ('Bearer token', token)]:
            root.append(label(title, 'author')); root.append(widget)
        error_label = label('', 'warning', True); root.append(error_label)
        actions = box()
        actions.append(button('Cancel', win.close))
        if self.session.wanted:
            actions.append(button('Disconnect', lambda: (self.session.disconnect(), win.close())))
        def connect():
            try:
                self.session.connect(url.get_text().strip(), name.get_text().strip(), token.get_text())
            except ValueError as e:
                error_label.set_text(str(e)); return
            self.drafts.clear(); self.reply = None; self.mention_ids = []
            self.editor.get_buffer().set_text('')
            self.room_id = ''; self.page_ids = None; self.cursor = None
            if self.preferences.data.get('remember'):
                try: self.preferences.save(url=url.get_text().strip(), name=name.get_text().strip())
                except OSError: self.session.error = 'Connected, but could not save connection preferences.'
            token.set_text(''); win.close()
        actions.append(button('Connect', connect, 'suggested-action'))
        if self.keyring.available:
            saved = button('Use saved session', lambda: None)
            def use_saved(*_):
                from .model import validate_url
                try: address = validate_url(url.get_text().strip())
                except ValueError as error:
                    error_label.set_text(str(error)); return
                saved.set_sensitive(False)
                def found(value, error):
                    if not win.get_visible(): return
                    saved.set_sensitive(True)
                    if error or not value:
                        error_label.set_text(error or 'No saved session for this address.'); return
                    token.set_text(value)
                    connect()
                self.keyring.lookup(address, found)
            saved.connect('clicked', use_saved)
            root.append(saved)
        root.append(actions)
        win.present()
        return False

    def join_room(self):
        rid = self.room_id
        def done(result, error):
            if not error:
                if rid in self.session.store.rooms:
                    self.session.store.rooms[rid]['_joined'] = True
                if rid == self.room_id:
                    self.load_history(latest=True)
        self.session.request('room_join', {'room_id': rid}, done)

    def new_thread(self):
        self.room_editor(parent=self.room_id)

    def mention_dialog(self):
        win, root = self.dialog('Mention someone')
        root.append(label('Choose a member to add a protocol mention.', 'dim-label', True))
        members = self.session.store.rooms.get(self.room_id, {}).get('members', [])[:100]
        scroll = Gtk.ScrolledWindow(min_content_height=200, max_content_height=400, propagate_natural_height=True)
        choices = box(True)
        for member in members:
            uid = member.get('user_id', '')
            def choose(uid=uid):
                if uid not in self.mention_ids:
                    self.mention_ids.append(uid)
                self.editor.get_buffer().insert_at_cursor('@' + uid + ' ')
                win.close(); self.editor.grab_focus()
            choices.append(button(safe_text(member.get('name') or uid) + ' · @' + safe_text(uid), choose))
        scroll.set_child(choices); root.append(scroll)
        root.append(button('Clear selected mentions', lambda: (self.mention_ids.clear(), self.update_send(), win.close())))
        win.present()

    def capture(self, filename):
        snapshot = Gtk.Snapshot()
        paintable = Gtk.WidgetPaintable.new(self)
        paintable.snapshot(snapshot, self.get_width(), self.get_height())
        node = snapshot.to_node()
        if node:
            texture = self.get_renderer().render_texture(node, None)
            texture.save_to_png(filename)
            print('Screenshot saved:', filename, flush=True)
        return False

    def close_requested(self, *_):
        self.session.disconnect()
        return False


class App(Adw.Application):
    def __init__(self, options):
        super().__init__(application_id='chat.apron.Native', flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.options = options
        action = Gio.SimpleAction.new('open-room', GLib.VariantType.new('s'))
        def open_room(action, value):
            win = self.get_active_window()
            if win and value.get_string() in win.session.store.rooms:
                win.present(); win.select_room(value.get_string())
        action.connect('activate', open_room)
        self.add_action(action)

    def do_activate(self):
        if self.get_active_window():
            self.get_active_window().present(); return
        style = Gtk.CssProvider()
        style.load_from_path(str(Path(__file__).with_name('style.css')))
        Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), style, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        Window(self, self.options)


def main():
    parser = argparse.ArgumentParser(description='Native Apron client')
    connection = parser.add_mutually_exclusive_group()
    connection.add_argument('--server', help='Connect as a guest to this ws(s) URL')
    connection.add_argument('--demo', action='store_true', help='Connect only to the local fixture at 127.0.0.1:18765')
    parser.add_argument('--screenshot', help='Save a GTK-rendered PNG after startup (testing only)')
    args = parser.parse_args()
    if args.server:
        from .model import validate_url
        try: validate_url(args.server)
        except ValueError as error: parser.error(str(error))
    return App(args).run([sys.argv[0]])


if __name__ == '__main__':
    raise SystemExit(main())
