"""Async libsoup transport. All callbacks run on GTK's GLib context."""
import json
import random
import time
import uuid
import gi

gi.require_version('Soup', '3.0')
from gi.repository import Soup, Gio, GLib
from .model import Store, MAX_FRAME, PAGE_SIZE, safe_text, validate_url, number
from .recovery import scan, reconcile_page


class Session:
    def __init__(self, changed=lambda: None):
        self.changed = changed
        self.store = Store()
        self.http = Soup.Session(timeout=20)
        self.ws = None
        self.cancel = None
        self.generation = 0
        self.pending = {}
        self.server = {}
        self.state = 'Disconnected'
        self.error = ''
        self.ready = False
        self.wanted = False
        self.attempt = 0
        self.retry_timer = self.ping_timer = self.hello_timer = 0
        self.awaiting_pong = False
        self.auth_started = False
        self.url = ''
        self.name = ''
        self.token = ''
        self.blocked_until = 0
        self.on_ready = lambda: None
        self.on_token = lambda: None
        self.on_message = lambda msg: None
        self.on_read = lambda: None

    @property
    def caps(self):
        return self.server.get('capabilities', [])

    def emit(self):
        self.changed()

    def connect(self, url, name='', token=''):
        validate_url(url)
        self.disconnect()
        self.store = Store()
        self.url, self.name, self.token = url, name, token
        self.wanted = True
        self.attempt = 0
        self.blocked_until = 0
        self._open()

    def _clear_timer(self, attr):
        timer = getattr(self, attr)
        if timer:
            GLib.source_remove(timer)
            setattr(self, attr, 0)

    def _open(self):
        self._clear_timer('retry_timer')
        self.generation += 1
        gen = self.generation
        self.server = {}
        self.ready = self.auth_started = False
        self.error = ''
        self.state = 'Connecting…'
        self.emit()
        self.cancel = Gio.Cancellable()
        msg = Soup.Message.new('GET', self.url)
        self.http.websocket_connect_async(msg, None, None, GLib.PRIORITY_DEFAULT,
                                          self.cancel, self._connected, gen)

    def _connected(self, http, result, gen):
        try:
            ws = http.websocket_connect_finish(result)
        except GLib.Error:
            if gen == self.generation:
                self._lost('Connection failed. Check the address and network.')
            return
        if gen != self.generation or not self.wanted:
            ws.close(1000, 'Cancelled')
            return
        self.ws = ws
        ws.set_max_incoming_payload_size(MAX_FRAME)
        ws.connect('message', self._received, gen)
        ws.connect('closed', lambda *_: self._lost('Connection closed.') if gen == self.generation else None)
        ws.connect('error', lambda *_: self._lost('WebSocket connection failed.') if gen == self.generation else None)
        self.state = 'Waiting for server…'
        self.hello_timer = GLib.timeout_add_seconds(20, self._hello_timeout, gen)
        self.emit()

    def _hello_timeout(self, gen):
        self.hello_timer = 0
        if gen == self.generation and not self.ready:
            self._lost('Server did not complete sign-in in time.')
        return False

    def _lost(self, reason):
        self.generation += 1  # makes late socket and response callbacks harmless
        self.ready = False
        self._clear_timer('hello_timer')
        self._clear_timer('ping_timer')
        if self.cancel:
            self.cancel.cancel()
        old, self.ws = self.ws, None
        if old and old.get_state() == Soup.WebsocketState.OPEN:
            old.close(1000, 'Reconnect')
        requests, self.pending = self.pending, {}
        for _, callback, timer in requests.values():
            GLib.source_remove(timer)
            callback(None, {'code': -1, 'message': 'Connection lost; delivery is uncertain. Check history before resending.'})
        self.error = reason
        if self.wanted:
            delay = max(min(30, 2 ** min(self.attempt, 5)) + random.random(), self.blocked_until - time.monotonic())
            self.attempt += 1
            self.state = f'Reconnecting in {delay:.0f}s…'
            self._clear_timer('retry_timer')
            self.retry_timer = GLib.timeout_add(max(1000, int(delay * 1000)), self._retry)
        else:
            self.state = 'Disconnected'
        self.emit()

    def _retry(self):
        self.retry_timer = 0
        if self.wanted:
            self._open()
        return False

    def disconnect(self):
        self.wanted = False
        self._clear_timer('retry_timer')
        self._lost('')
        self.token = ''

    def notify(self, method, params=None):
        if not self.ws or self.ws.get_state() != Soup.WebsocketState.OPEN:
            return
        frame = {'method': method}
        if params is not None:
            frame['params'] = params
        self.ws.send_text(json.dumps(frame))

    def request(self, method, params, callback=lambda result, error: None):
        if len(self.pending) >= 64 or not self.ws or self.ws.get_state() != Soup.WebsocketState.OPEN:
            callback(None, {'code': -1, 'message': 'Not connected or too many requests.'})
            return
        if time.monotonic() < self.blocked_until:
            callback(None, {'code': -32002, 'message': 'Server asked us to wait. Try again shortly.'})
            return
        rid = uuid.uuid4().hex
        frame = json.dumps({'method': method, 'id': rid, 'params': params})
        if len(frame.encode()) > MAX_FRAME:
            callback(None, {'code': -32003, 'message': 'Message is too large.'})
            return
        timer = GLib.timeout_add_seconds(20, self._timeout, rid)
        self.pending[rid] = (method, callback, timer)
        self.ws.send_text(frame)

    def _timeout(self, rid):
        pending = self.pending.pop(rid, None)
        if pending:
            pending[1](None, {'code': -1, 'message': 'Request timed out; delivery may be uncertain. Check history before retrying.'})
        return False

    def _received(self, ws, kind, data, gen):
        if gen != self.generation or kind != Soup.WebsocketDataType.TEXT:
            return
        try:
            raw = data.get_data()
            if len(raw) > MAX_FRAME:
                raise ValueError('Frame too large')
            frame = json.loads(raw)
            if not isinstance(frame, dict):
                raise ValueError('Expected object')
            self.frame(frame)
        except (ValueError, TypeError, KeyError, AttributeError, OverflowError):
            self.error = 'Ignored malformed server data.'
            self.emit()

    def frame(self, frame):
        if 'error' in frame and 'id' not in frame:
            error = frame['error']
            if error.get('code') == -32002:
                self.blocked_until = time.monotonic() + max(1, min(float(error.get('data', {}).get('retry_after', 30)), 86400))
            if error.get('code') == -32001:
                self.wanted = False
            self._lost(safe_text(error.get('message', 'Server closed this connection'), 500))
            return
        if 'id' in frame and 'method' not in frame:
            pending = self.pending.pop(frame['id'], None)
            if pending:
                GLib.source_remove(pending[2])
                error = frame.get('error')
                if error:
                    if error.get('code') == -32002:
                        wait = error.get('data', {}).get('retry_after', 30)
                        self.blocked_until = time.monotonic() + max(1, min(float(wait), 86400))
                    self.error = safe_text(error.get('message', 'Server rejected request'), 500)
                pending[1](frame.get('result', {}), error)
                self.emit()
            return
        method, p = frame.get('method'), frame.get('params', {})
        if method == 'server':
            self.server = p
            self._clear_timer('ping_timer')
            self.awaiting_pong = False
            interval = p.get('ping')
            if isinstance(interval, (int, float)) and 0.1 <= interval <= 86400:
                self.ping_timer = GLib.timeout_add(int(interval * 1000), self._ping)
            if p.get('apron') != 8:
                self.wanted = False
                self._lost('This client supports Apron protocol 8.')
                return
            if not self.auth_started:
                scheme = 'token' if self.token else 'guest'
                if scheme not in p.get('auth', []) + p.get('signup', []):
                    self.wanted = False
                    self._lost('This server needs another sign-in method. Guest and token are supported.')
                    return
                self.auth_started = True
                params = {'scheme': scheme, 'agent': 'apron-native/0.1', 'name': self.name}
                if self.token:
                    params['token'] = self.token
                self.state = 'Signing in…'
                self.request('auth', params, self._authenticated)
        elif method == 'pong':
            self.awaiting_pong = False
        elif method == 'message':
            if 'message_id' not in p:
                self.error = safe_text(p.get('body', {}).get('text', ''), 500)
            elif self.store.message(p, live=self.ready):
                self.on_message(p)
        elif method == 'room_update':
            self.store.listing(p)
        elif method == 'reactions':
            self.store.reaction(p)
        elif method == 'user':
            self.store.user(p.get('new', {}))
            if p.get('you'):
                self.store.you.update(p['you'])
                self.store.user(p['you'])
        elif method == 'activity' and p.get('from', {}).get('user_id') == self.store.you.get('user_id'):
            if p.get('read_message_id'):
                self.store.mark_read(p.get('room_id', ''), p['read_message_id'])
                self.on_read()
        self.emit()

    def _ping(self):
        if self.awaiting_pong:
            self.ping_timer = 0
            self._lost('Server stopped responding.')
            return False
        self.awaiting_pong = True
        self.notify('ping')
        return True

    def _authenticated(self, result, error):
        if error and error.get('code') in (-1, -32002):
            # A transport failure while auth is pending must not disable reconnect.
            if self.ws is not None:
                self._lost(error.get('message', 'Sign-in interrupted.'))
            return
        if error or not result.get('you', {}).get('user_id'):
            self.wanted = False  # never silently fall back from an invalid token
            self._lost(self.error or 'Sign-in failed.')
            return
        old_you = self.store.you.get('user_id')
        old_reads = self.store.reads if old_you == result['you']['user_id'] else {}
        self.store = Store()  # fresh bounded window after reconnect, no stale cache
        self.store.reads = old_reads
        self.store.you = result['you']
        self.store.user(result['you'])
        self.token = result.get('token', self.token)  # memory only, including rotation
        self.ready = True
        if result.get('token'): self.on_token()
        self.attempt = 0
        self.state = 'Connected'
        self._clear_timer('hello_timer')
        if 'rooms' in self.caps:
            self.list_rooms(callback=lambda *_: self.on_ready())
        else:
            self.on_ready()
        self.emit()

    def list_rooms(self, parent=None, callback=lambda *_: None):
        params = {'filter': 'all', 'members': True}
        if parent is not None:
            params['parent_room_id'] = parent
        def done(result, error):
            if not error:
                self.store.listing(result)
            callback(result, error)
            self.emit()
        self.request('room_list', params, done)

    def history(self, rid, before=None, callback=lambda *_: None, valid=lambda: True, recover=False):
        params = {'limit': PAGE_SIZE}
        if rid:
            params['room_id'] = rid
        if before is not None:
            params['before'] = before
        def finish(result, error):
            if not valid(): return
            cursor = None
            if not error:
                try: cursor = self.store.history(rid, result)
                except (ValueError, TypeError, KeyError, AttributeError):
                    error = {'code': -1, 'message': 'Invalid history response.'}
            if error:
                self.error = error['message']
                if 'retention' in self.error.lower(): self.store.discard_room_history(rid)
            callback(result, error, cursor)
            self.emit()
        def received(result, error):
            if not valid(): return
            if error: finish(result, error); return
            if before is not None:
                reconcile_page(self, rid, result, finish, valid)
            elif recover and rid in self.store.reads:
                try:
                    horizon = number(result['latest_log_id'])
                    lower = number(result['history_log_id']) if result.get('history_log_id') is not None else horizon + 1
                    start = self.store.reads[rid] + 1
                    if lower > start:
                        self.error = 'Some unread history has expired. Counts cover available history only.'
                        start = lower
                except (ValueError, KeyError, TypeError):
                    finish(None, {'code': -1, 'message': 'Invalid recovery bounds.'}); return
                def apply(page):
                    self.store.listing(page)
                    for msg in page.get('messages', []):
                        self.store.message(msg)
                        current = self.store.messages.get(msg['message_id'], msg)
                        if current['room_id'] == rid: self.store.note_unread(current)
                    for record in page.get('reactions', []): self.store.reaction(record)
                def recovered(error):
                    if error: self.error = 'Unread recovery incomplete. ' + error['message']
                    finish(result, None)  # latest page itself is still valid
                scan(self, rid, start, horizon, apply, recovered, valid)
            else: finish(result, None)
        self.request('history', params, received)

    def send(self, rid, text, reply=None, mentions=None, callback=lambda *_: None):
        params = {'body': {'text': text, 'format': 'plain', 'mentions': mentions or []}}
        if rid:
            params['room_id'] = rid
        if reply:
            params['reply_to'] = {'message_id': reply}
        self.request('message', params, callback)
