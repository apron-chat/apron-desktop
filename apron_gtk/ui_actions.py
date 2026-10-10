"""Native dialogs and discoverable room/message actions."""
from copy import deepcopy
import gi
from gi.repository import Gtk, Adw, GLib, Gio
from .actions import message_save, same_snapshot
from .model import safe_text


class DailyActions:
    def menu(self, items, title='More actions'):
        from .app import box, button
        control = Gtk.MenuButton(icon_name='view-more-symbolic', tooltip_text=title)
        control.add_css_class('flat')
        control.set_always_show_arrow(False)
        control.update_property([Gtk.AccessibleProperty.LABEL], [title])
        def populate(menu_button, *_):
            popover = Gtk.Popover()
            content = box(True, 4, 8)
            for name, action, enabled in (items() if callable(items) else items):
                def invoke(action=action):
                    popover.popdown(); action()
                item = button(name, invoke, 'flat')
                item.set_sensitive(enabled)
                content.append(item)
            popover.set_child(content)
            if title == 'Message actions':
                def shown(*_): self.message_menu_open = True
                popover.connect('show', shown)
            menu_button.set_popover(popover)
            def closed(*_):
                if title == 'Message actions':
                    self.message_menu_open = False; self.queue_render()
                def release():
                    if menu_button.get_popover() == popover and not popover.get_visible():
                        menu_button.set_popover(None)
                    return False
                GLib.idle_add(release)
            popover.connect('closed', closed)
        control.set_create_popup_func(populate)
        control.set_size_request(36, 36)
        return control

    def submit_dialog(self, win, root, method, params, success=lambda result: None, guard=lambda: None, title='Save'):
        from .app import label, button, box
        generation = self.session.generation
        error_label = label('', 'warning', True)
        root.append(error_label)
        def submit():
            if not control.get_sensitive(): return
            try:
                if not self.session.ready or self.session.generation != generation:
                    raise ValueError('Connection changed. Close and reopen this dialog before trying again.')
                guard()
                payload = params()
            except ValueError as error:
                error_label.set_text(str(error)); return
            control.set_sensitive(False)
            control.set_label('Working…')
            def done(result, error):
                control.set_sensitive(True); control.set_label(title)
                if error:
                    error_label.set_text(safe_text(error.get('message', 'Request failed'), 600)); return
                try: success(result)
                except ValueError as error:
                    control.set_sensitive(False)
                    error_label.set_text(str(error)); return
                win.close(); self.queue_render()
            self.session.request(method, payload, done)
        control = button(title, submit, 'suggested-action')
        footer = box(spacing=8)
        footer.set_halign(Gtk.Align.END); footer.add_css_class('dialog-actions')
        footer.append(button('Cancel', win.close, 'flat')); footer.append(control)
        root.append(footer)
        return control

    def confirm_action(self, title, description, method, params, success=lambda _: None, guard=lambda: None):
        from .app import label
        win, root = self.dialog(title)
        root.append(label(description, None, True))
        control = self.submit_dialog(win, root, method, lambda: params, success, guard, title)
        control.remove_css_class('suggested-action'); control.add_css_class('destructive-action')
        win.present()
        return win

    def message_guard(self, msg):
        if not same_snapshot(self.session.store, msg):
            raise ValueError('This message changed or left the cache. Reopen the action on its current version.')

    def message_menu(self, msg):
        own = msg.get('from', {}).get('user_id') == self.session.store.you.get('user_id')
        editable = self.session.ready and 'edit' in self.session.caps and own and not msg.get('deleted')
        return self.menu([
            ('Reply', lambda: self.set_reply(msg), not msg.get('deleted')),
            ('React…', lambda: self.reaction_dialog(msg), self.session.ready and 'reactions' in self.session.caps and not msg.get('deleted')),
            ('Copy text', lambda: self.get_display().get_clipboard().set(safe_text(msg.get('body', {}).get('text', ''))), not msg.get('deleted')),
            ('Copy message ID', lambda: self.get_display().get_clipboard().set(msg['message_id']), True),
            ('Start a thread', lambda: self.room_editor(parent=msg['room_id'], reply=msg), self.session.ready and 'rooms' in self.session.caps),
            ('Edit message…', lambda: self.edit_message(msg), editable),
            ('Move message…', lambda: self.move_message(msg), editable),
            ('Delete message…', lambda: self.confirm_action('Delete message', 'Delete this message for everyone? The server may retain older history.',
                    'message', message_save(msg, deleted=True), guard=lambda: self.message_guard(msg)), editable),
        ], 'Message actions')

    def edit_message(self, msg):
        from .app import label
        win, root = self.dialog('Edit message')
        root.append(label('Attachments, mentions, formatting, and the reply are preserved.', 'dim-label', True))
        editor = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        editor.get_buffer().set_text(msg.get('body', {}).get('text', ''))
        scroll = Gtk.ScrolledWindow(min_content_height=160, max_content_height=320, propagate_natural_height=True)
        scroll.set_child(editor); root.append(scroll)
        def payload():
            buf = editor.get_buffer()
            return message_save(msg, text=buf.get_text(buf.get_start_iter(), buf.get_end_iter(), False))
        self.submit_dialog(win, root, 'message', payload, guard=lambda: self.message_guard(msg))
        win.present(); editor.grab_focus()
        return win

    def move_message(self, msg):
        from .app import label
        win, root = self.dialog('Move message')
        rooms = list(self.session.store.rooms.values())
        root.append(label('Choose the destination room or thread.', None, True))
        picker = Gtk.DropDown.new_from_strings([safe_text(r.get('title') or r['room_id']) for r in rooms])
        root.append(picker)
        self.submit_dialog(win, root, 'message', lambda: message_save(msg, room_id=rooms[picker.get_selected()]['room_id']),
                           guard=lambda: self.message_guard(msg), title='Move')
        win.present()

    def reaction_dialog(self, msg):
        from .app import box, button, label
        win, root = self.dialog('React to message')
        root.append(label('Choose an emoji to add or remove your reaction.', 'dim-label', True))
        row = box()
        for emoji in ('👍', '❤️', '🎉', '👀', '✅', '🤔'):
            row.append(button(emoji, lambda e=emoji: self.toggle_reaction(msg, e, win)))
        root.append(row)
        entry = Gtk.Entry(placeholder_text='Another emoji')
        root.append(entry)
        root.append(button('Toggle reaction', lambda: self.toggle_reaction(msg, entry.get_text().strip(), win)))
        win.present()

    def toggle_reaction(self, msg, emoji, win=None):
        if not emoji or len(emoji) > 16 or any(c.isspace() for c in emoji):
            self.session.error = 'Choose one short emoji sequence.'; self.queue_render(); return
        key = (msg['message_id'], self.session.store.you.get('user_id'))
        if key in self.reaction_pending: return
        current = set(self.session.store.reactions.get(key, (0, []))[1])
        current.symmetric_difference_update({emoji})
        self.reaction_pending.add(key)
        def done(result, error):
            self.reaction_pending.discard(key)
            if not error and win: win.close()
            self.queue_render()
        self.session.request('reactions', {'message_id': msg['message_id'], 'emojis': sorted(current)}, done)

    def room_editor(self, parent=None, edit=None, reply=None):
        from .app import label
        title = 'Edit room' if edit else 'Start a thread' if parent else 'Create a room'
        win, root = self.dialog(title)
        name = Gtk.Entry(placeholder_text='Title', text=safe_text((edit or {}).get('title') or (reply or {}).get('body', {}).get('text', '').split('\n')[0], 160))
        description = Gtk.Entry(placeholder_text='Description or summary', text=safe_text((edit or {}).get('description', ''), 1000))
        for caption, control in [('Title', name), ('Description', description)]:
            root.append(label(caption, 'author')); root.append(control)
        private = Gtk.CheckButton(label='Private room — members only')
        if not parent and not edit: root.append(private)
        intended_private = bool(parent and self.session.store.rooms.get(parent, {}).get('private'))
        if intended_private: root.append(label('This thread must remain private like its parent.', None, True))
        def params():
            if not name.get_text().strip(): raise ValueError('A title is required.')
            p = {'title': name.get_text().strip(), 'description': description.get_text()}
            if edit:
                p['room_id'] = edit['room_id']
                if edit.get('ext'): p['ext'] = deepcopy(edit['ext'])
            elif parent: p['parent_room_id'] = parent
            elif private.get_active(): p['private'] = True
            return p
        def done(result):
            rid = result['room_id']
            if not edit and (intended_private or private.get_active()) and not self.session.store.rooms.get(rid, {}).get('private'):
                raise ValueError('The server did not confirm privacy. The room was created, but was not opened. Do not post sensitive content.')
            self.select_room(rid)
            if reply: self.set_reply(reply)
        self.submit_dialog(win, root, 'room_set', params, done, title='Save' if edit else 'Create')
        win.present(); name.grab_focus()
        return win

    def room_details(self):
        from .app import label, box, button
        rid = self.room_id; room = self.session.store.rooms.get(rid, {})
        win, root = self.dialog(safe_text(room.get('title') or rid))
        root.append(label(('Private room' if room.get('private') else 'Room') + ' · ' + rid, 'dim-label', True))
        root.append(label(room.get('description', ''), None, True))
        available = 'rooms' in self.session.caps and self.session.ready
        if available:
            root.append(button('Edit title and description…', lambda: (win.close(), self.room_editor(edit=room))))
            root.append(button('New thread…', lambda: (win.close(), self.room_editor(parent=rid))))
            if room.get('_joined'):
                root.append(button('Leave room…', lambda: (win.close(), self.confirm_action('Leave room', 'Stop receiving new messages in this room? Its threads keep their own membership. Private rooms may require an invitation to return.',
                    'room_leave', {'room_id': rid}, lambda _: self.session.list_rooms()))))
        members = room.get('members', [])
        count = room.get('member_count', len(members))
        root.append(label(f'Members · {len(members)} listed of {count}', 'author'))
        scroll = Gtk.ScrolledWindow(min_content_height=100, max_content_height=280, propagate_natural_height=True)
        rows = box(True)
        for member in members[:200]:
            uid = member.get('user_id', '')
            current = self.session.store.users.get(uid, member)
            line = box()
            name = label(safe_text(current.get('name') or uid) + ' (@' + safe_text(uid) + ')', None, True)
            name.set_hexpand(True); line.append(name)
            line.append(button('Copy ID', lambda u=uid: self.get_display().get_clipboard().set(u)))
            if available and room.get('_joined') and uid != self.session.store.you.get('user_id'):
                line.append(button('Remove…', lambda u=uid: (win.close(), self.confirm_action('Remove member', 'Remove @' + safe_text(u) + ' from this room?', 'room_leave', {'room_id': rid, 'user_id': u}))))
            rows.append(line)
        scroll.set_child(rows); root.append(scroll)
        if available and room.get('_joined'):
            entry = Gtk.Entry(placeholder_text='Exact user ID to invite')
            root.append(entry)
            def params():
                if not entry.get_text().strip(): raise ValueError('Enter a user ID, not a display name.')
                return {'room_id': rid, 'user_id': entry.get_text().strip()}
            self.submit_dialog(win, root, 'room_join', params, lambda _: self.session.list_rooms(), title='Add member')
        else: root.append(button('Close', win.close))
        win.present()

    def search_messages(self):
        from .app import box, label, button
        win, root = self.dialog('Find loaded messages')
        root.append(label('Search the in-memory cache only, across rooms. Older or unloaded history is not searched. Up to 100 results.', 'dim-label', True))
        query = Gtk.SearchEntry(placeholder_text='Text or author')
        root.append(query)
        results = box(True)
        scroll = Gtk.ScrolledWindow(min_content_height=250, max_content_height=420, propagate_natural_height=True)
        scroll.set_child(results); root.append(scroll)
        def search(*_):
            from .app import clear
            clear(results)
            q = query.get_text().casefold().strip()
            if not q: return
            matches = [m for m in self.session.store.messages.values() if not m.get('deleted') and q in
                       (m.get('body', {}).get('text', '') + ' ' + m.get('from', {}).get('name', '')).casefold()]
            for msg in matches[-100:]:
                text = safe_text(msg.get('body', {}).get('text', ''), 90).replace('\n', ' ')
                rid = msg['room_id']
                def choose(msg=msg):
                    self.drafts[self.room_id] = (self.text(), self.reply, self.mention_ids[:])
                    self.room_id = msg['room_id']; self.load_generation += 1; self.loading = False
                    self.older = True; self.page_ids = {msg['message_id']}; self.cursor = None
                    draft, self.reply, self.mention_ids = self.drafts.get(self.room_id, ('', None, []))
                    self.editor.get_buffer().set_text(draft)
                    self.session.error = 'Search result from loaded history. Choose Latest to return to the conversation.'
                    win.close(); self.queue_render()
                results.append(button(safe_text(self.session.store.rooms.get(rid, {}).get('title') or rid) + ' · ' + text, choose))
            if not matches: results.append(label('No loaded messages match.'))
        query.connect('search-changed', search)
        win.present(); query.grab_focus()

    def profile_dialog(self):
        from .app import label
        win, root = self.dialog('Your profile')
        root.append(label('@' + safe_text(self.session.store.you.get('user_id', '')), 'dim-label'))
        name = Gtk.Entry(text=safe_text(self.session.store.you.get('name', '')), placeholder_text='Display name')
        root.append(label('Display name', 'author')); root.append(name)
        statuses = ['online', ''] + [s for s in self.session.server.get('status', []) if s in ('dnd','invisible')]
        status = Gtk.DropDown.new_from_strings([{'online':'Online', '':'No status', 'dnd':'Do not disturb', 'invisible':'Invisible'}[s] for s in statuses])
        status.set_selected(statuses.index(self.session.store.you.get('status', 'online')) if self.session.store.you.get('status', 'online') in statuses else 0)
        if 'status' in self.session.caps:
            root.append(label('Presence', 'author')); root.append(status)
        def params():
            p = {'name': name.get_text()}
            if 'status' in self.session.caps: p['status'] = statuses[status.get_selected()]
            return p
        def done(result):
            if result.get('you'):
                self.session.store.you = result['you']; self.session.store.user(result['you'], complete=True)
            self.session.name = name.get_text()
        self.submit_dialog(win, root, 'me', params, done)
        win.present()

    def mute_dialog(self, rid=None):
        from .app import label
        title='Mute this room' if rid else 'Notification mutes'
        win, root=self.dialog(title)
        active=self.session.presence.muted(rid or '', self.session.store.rooms)
        root.append(label('Notifications are currently muted.' if active else 'Notifications are currently enabled.', 'dim-label', True))
        root.append(label('Room mutes include its threads. A global mute or Do Not Disturb can still silence an unmuted room. The server confirms timing and may treat timed mutes as indefinite.', 'dim-label', True))
        choices=[False,900,3600,True]
        choice=Gtk.DropDown.new_from_strings(['Unmute','15 minutes','1 hour','Until I unmute'])
        choice.set_selected(0 if active else 2);root.append(choice)
        def params():
            if 'status' not in self.session.caps:raise ValueError('This server does not support notification mutes.')
            return {'mute':choices[choice.get_selected()], **({'room_id':rid} if rid else {})}
        self.submit_dialog(win,root,'status',params,title='Apply')
        win.present();return win

    def preferences_dialog(self):
        from .app import label, button
        win, root = self.dialog('Preferences')
        root.append(label('Appearance', 'author'))
        theme = Gtk.DropDown.new_from_strings(['System', 'Light', 'Dark'])
        choices = ['system','light','dark']
        theme.set_selected(choices.index(self.preferences.data.get('theme', 'system')))
        root.append(theme)
        remember = Gtk.CheckButton(label='Remember server address and display name', active=self.preferences.data.get('remember', False))
        notifications = Gtk.CheckButton(label='Notify about mentions while this window is inactive', active=self.preferences.data.get('notifications', False))
        local = Gtk.CheckButton(label='Save drafts and read markers on this device', active=self.preferences.data.get('local_state', False))
        secure = Gtk.CheckButton(label='Save session tokens in the system keyring', active=self.preferences.data.get('secure_session', False))
        secure.set_sensitive(self.keyring.available)
        for toggle in (remember, notifications, local, secure):
            child = toggle.get_child()
            if isinstance(child, Gtk.Label):
                child.set_wrap(True); child.set_max_width_chars(40); child.set_xalign(0)
            root.append(toggle)
        root.append(label('Local drafts/read markers are opt-in plaintext, scoped to server and user, capped at 1 MiB and 8 accounts. Message history is never saved. Disabling storage removes saved local state. Notification text omits message content. Tokens are saved only when keyring storage is enabled; no plaintext file fallback exists. A Secret Service provider must be installed and unlocked.', 'dim-label', True))
        error_label = label('', 'warning', True); root.append(error_label)
        def save():
            try:
                self.preferences.save(theme=choices[theme.get_selected()], remember=remember.get_active(), notifications=notifications.get_active(), secure_session=secure.get_active(), local_state=local.get_active(), url=self.session.url, name=self.session.name)
            except OSError:
                error_label.set_text('Could not save preferences. Check the configuration-directory permissions.'); return
            if local.get_active(): self.persist_state()
            else:
                if self.state_timer: GLib.source_remove(self.state_timer); self.state_timer = 0
                self.state_writer.submit(reset=True)
            self.apply_theme(); self.save_secure_session(); win.close()
        from .app import box
        footer = box(spacing=8); footer.set_halign(Gtk.Align.END); footer.add_css_class('dialog-actions')
        footer.append(button('Cancel', win.close, 'flat'))
        footer.append(button('Save preferences', save, 'suggested-action'))
        if self.keyring.available and self.session.url:
            root.append(button('Forget saved session for this server', lambda: (secure.set_active(False), self.forget_secure_session(self.session.url, lambda value, error: error_label.set_text(error or 'Stored session removed. Current connection remains open.')))))
        def reset():
            confirm, content = self.dialog('Reset local drafts and read markers')
            content.append(label('Remove all saved drafts/read markers for every server and clear current drafts/read markers? Server messages, preferences, and keyring tokens are kept.', None, True))
            problem = label('', 'warning', True); content.append(problem)
            def confirmed():
                try:
                    self.reset_local_state(); local.set_active(False)
                    error_label.set_text('Drafts cleared; saved-data removal queued. Storage is off.')
                    confirm.close()
                except OSError: problem.set_text('Could not change storage settings. Check directory permissions.')
            actions = box(spacing=8); actions.set_halign(Gtk.Align.END); actions.add_css_class('dialog-actions')
            actions.append(button('Cancel', confirm.close, 'flat'))
            actions.append(button('Reset local data', confirmed, 'destructive-action'))
            content.append(actions); confirm.present()
        root.append(button('Reset local drafts and read markers…', reset))
        root.append(footer)
        win.present()

    def apply_theme(self):
        scheme = {'system': Adw.ColorScheme.DEFAULT, 'light': Adw.ColorScheme.FORCE_LIGHT, 'dark': Adw.ColorScheme.FORCE_DARK}
        Adw.StyleManager.get_default().set_color_scheme(scheme[self.preferences.data.get('theme', 'system')])

    def shortcuts_dialog(self):
        from .app import label, button
        win, root = self.dialog('Keyboard shortcuts')
        for text in ['Ctrl+K — Find a room', 'Ctrl+F — Search loaded messages', 'Ctrl+L — Focus composer', 'Ctrl+Page Up / Down — Earlier / latest messages', 'Shift+F10 — Actions for the focused message', 'Ctrl+Enter — Send', 'Alt+Up — Open parent thread room', 'Alt+N — Next unread room', 'Escape — Cancel reply', 'Tab / Shift+Tab — Move between controls']:
            root.append(label(text))
        root.append(button('Close', win.close)); win.present()

    def next_unread(self):
        rooms = list(self.session.store.rooms)
        if self.room_id in rooms:
            index = rooms.index(self.room_id) + 1; rooms = rooms[index:] + rooms[:index]
        target = next((rid for rid in rooms if self.session.store.unread.get(rid)), None)
        if target: self.select_room(target)

    def save_secure_session(self):
        if not self.preferences.data.get('secure_session') or not self.session.token: return
        # Serialize writes so a slow older token write cannot overwrite a rotation.
        self.keyring_jobs[self.session.url] = (self.session.token, self.session.generation, lambda *_: None)
        while len(self.keyring_jobs) > 8:
            self.keyring_jobs.pop(next(iter(self.keyring_jobs)))
        self._save_next_token()

    def _save_next_token(self):
        if self.keyring_busy or not self.keyring_jobs: return
        url = next(iter(self.keyring_jobs))
        token, generation, callback = self.keyring_jobs.pop(url)
        self.keyring_busy = True
        def done(value, error):
            self.keyring_busy = False
            if error and generation == self.session.generation:
                self.session.error = error; self.queue_render()
            callback(value, error)
            self._save_next_token()
        if token is None: self.keyring.forget(url, done)
        else: self.keyring.store(url, token, done)

    def forget_secure_session(self, url, callback):
        self.preferences.data['secure_session'] = False
        try: self.preferences.save(secure_session=False)
        except OSError:
            callback(None, 'Could not persist the preference; the current session will stop saving tokens.')
        self.keyring_jobs[url] = (None, self.session.generation, callback)
        self._save_next_token()
