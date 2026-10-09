"""Pure protocol state. No network, GTK, persistence, or credentials."""
from collections import OrderedDict
from copy import deepcopy
import re
import json
from urllib.parse import urlsplit

PAGE_SIZE = 50
MAX_MESSAGES = 2000
MAX_CACHE_BYTES = 8 * 1024 * 1024
MAX_ROOMS = 256
MAX_USERS = 2048
MAX_FRAME = 256 * 1024


def number(value):
    if not isinstance(value, str) or not re.fullmatch(r'[0-9]{1,64}', value):
        raise ValueError('Invalid decimal log identifier')
    return int(value)


def safe_text(value, limit=32000):
    """Plain GTK text only; remove control and bidi override characters."""
    if not isinstance(value, str):
        return ''
    return ''.join(c for c in value[:limit] if c in '\n\t' or
                   (ord(c) >= 32 and not 0x7f <= ord(c) <= 0x9f
                    and c not in '\u202a\u202b\u202c\u202d\u202e\u2066\u2067\u2068\u2069'))


def validate_url(url):
    u = urlsplit(url)
    if u.scheme not in ('ws', 'wss') or not u.hostname or u.username or u.password or u.fragment or u.query:
        raise ValueError('Use a ws:// or wss:// address without credentials, query, or fragment')
    if u.scheme == 'ws' and u.hostname not in ('localhost', '127.0.0.1', '::1'):
        raise ValueError('Remote servers require encrypted wss://')
    return url


class Store:
    def __init__(self):
        self.rooms = OrderedDict()
        self.messages = OrderedDict()
        self.users = OrderedDict()
        self.unread = {}
        self.mentions = {}
        self.reads = {}
        self.unread_ids = {}
        self.mention_ids = {}
        self.you = {}
        self.evicted = False
        self.message_sizes = {}
        self.cache_bytes = 0

    def user(self, user):
        if not isinstance(user, dict) or not isinstance(user.get('user_id'), str):
            return
        key = user['user_id']
        known = {k: safe_text(v, 300) for k, v in user.items() if k in ('user_id', 'name', 'status') and isinstance(v, str)}
        self.users[key] = {**self.users.get(key, {}), **known}
        self.users.move_to_end(key)
        while len(self.users) > MAX_USERS:
            self.users.popitem(last=False)

    def room(self, room, joined=None):
        if not isinstance(room, dict) or not isinstance(room.get('room_id'), str):
            return
        key = room['room_id']
        old = self.rooms.get(key, {})
        if 'log_id' in room and 'log_id' in old and number(room['log_id']) < number(old['log_id']):
            return
        # Room records are snapshots, except local subscription metadata.
        self.rooms[key] = {**room, '_joined': old.get('_joined', False) if joined is None else joined}
        for user in room.get('members', []):
            self.user(user)
        while len(self.rooms) > MAX_ROOMS:
            rid, _ = self.rooms.popitem(last=False)
            self.unread.pop(rid, None); self.mentions.pop(rid, None); self.reads.pop(rid, None)
            self.unread_ids.pop(rid, None); self.mention_ids.pop(rid, None)

    def listing(self, result):
        for key in ('joined', 'not_joined', 'updated', 'rooms'):
            for room in result.get(key, []):
                self.room(room, True if key == 'joined' else False if key == 'not_joined' else None)
        for room in result.get('left', []):
            rid = room.get('room_id')
            if rid in self.rooms:
                self.rooms[rid]['_joined'] = False
        for user in result.get('users', []):
            self.user(user)

    def message(self, msg, live=False, depth=0):
        if not isinstance(msg, dict) or not isinstance(msg.get('message_id'), str):
            return False
        mid = msg['message_id']; number(mid)
        log = number(msg.get('log_id'))
        rid = msg.get('room_id')
        if not isinstance(rid, str):
            return False
        old = self.messages.get(mid)
        if old and number(old['log_id']) >= log:
            return False
        if depth == 0 and isinstance(msg.get('reply_to'), dict) and 'log_id' in msg['reply_to']:
            self.message(msg['reply_to'], depth=1)
        self.cache_bytes -= self.message_sizes.get(mid, 0)
        size = len(json.dumps(msg).encode())
        self.message_sizes[mid] = size
        self.cache_bytes += size
        self.messages[mid] = deepcopy(msg)
        self.messages.move_to_end(mid)
        if rid not in self.rooms:
            self.room({'room_id': rid})
        if live and not old and log == number(mid) and msg.get('from', {}).get('user_id') != self.you.get('user_id'):
            if number(mid) > self.reads.get(rid, 0):
                ids = self.unread_ids.setdefault(rid, set())
                ids.add(mid)
                if len(ids) > 999:
                    ids.remove(min(ids, key=number))
                self.unread[rid] = len(ids)
                if self.you.get('user_id') in msg.get('body', {}).get('mentions', []):
                    ids = self.mention_ids.setdefault(rid, set())
                    ids.add(mid)
                    if len(ids) > 999:
                        ids.remove(min(ids, key=number))
                    self.mentions[rid] = len(ids)
        while len(self.messages) > MAX_MESSAGES or self.cache_bytes > MAX_CACHE_BYTES:
            evicted, _ = self.messages.popitem(last=False)
            self.cache_bytes -= self.message_sizes.pop(evicted)
            self.evicted = True
        return True

    def timeline(self, rid):
        return sorted((m for m in self.messages.values() if m['room_id'] == rid), key=lambda m: number(m['message_id']))

    def mark_read(self, rid, mid):
        if rid not in self.rooms:
            return
        self.reads[rid] = max(self.reads.get(rid, 0), number(mid))
        for ids_map, counts in ((self.unread_ids, self.unread), (self.mention_ids, self.mentions)):
            ids_map[rid] = {i for i in ids_map.get(rid, set()) if number(i) > self.reads[rid]}
            counts[rid] = len(ids_map[rid])

    def history(self, rid, result):
        latest = number(result['latest_log_id'])
        lower = latest + 1 if result.get('history_log_id') is None else number(result['history_log_id'])
        # Never display cached snapshots which retention can no longer validate.
        for mid, msg in list(self.messages.items()):
            if msg['room_id'] == rid and number(msg['log_id']) < lower:
                del self.messages[mid]
                self.cache_bytes -= self.message_sizes.pop(mid)
        self.listing(result)
        for msg in result.get('messages', []):
            if number(msg['log_id']) >= lower:
                self.message(msg)
        first = result.get('first_log_id')
        return str(number(first) - 1) if result.get('more') and first and number(first) > lower else None
