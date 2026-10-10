"""Bounded ephemeral presence. Deadlines use monotonic time, never wall clocks."""
from collections import OrderedDict
import time
import math


class Presence:
    def __init__(self):
        self.typing = OrderedDict()
        self.mutes = OrderedDict()

    def activity(self, record, own, now=None):
        now = time.monotonic() if now is None else now
        rid, uid, seconds = record.get('room_id'), record.get('from', {}).get('user_id'), record.get('typing')
        if not isinstance(rid, str) or not isinstance(uid, str) or uid == own or not isinstance(seconds, (int, float)) or isinstance(seconds, bool): return
        if not math.isfinite(seconds): return
        key = (rid[:256], uid[:256])
        if seconds <= 0: self.typing.pop(key, None)
        else:
            self.typing[key] = now + min(seconds, 30)
            self.typing.move_to_end(key)
            while len(self.typing) > 256: self.typing.popitem(last=False)

    def status(self, record, now=None):
        now = time.monotonic() if now is None else now
        rid, mute = record.get('room_id', ''), record.get('mute')
        if not isinstance(rid, str) or len(rid)>256 or mute is None: return
        if mute is False: self.mutes.pop(rid, None)
        elif mute is True: self.mutes[rid] = None
        elif isinstance(mute, int) and mute > 0: self.mutes[rid] = now + min(mute, 31536000)
        else: return
        if rid in self.mutes: self.mutes.move_to_end(rid)
        while len(self.mutes)>256: self.mutes.popitem(last=False)

    def expire(self, now=None):
        now = time.monotonic() if now is None else now
        for mapping in (self.typing, self.mutes):
            for key, until in list(mapping.items()):
                if until is not None and until <= now: del mapping[key]

    def deadline(self):
        values = list(self.typing.values()) + [v for v in self.mutes.values() if v is not None]
        return min(values) if values else None

    def muted(self, rid, rooms):
        if '' in self.mutes: return True
        visited=set()
        while rid and rid not in visited and len(visited)<256:
            if rid in self.mutes: return True
            visited.add(rid); rid=rooms.get(rid, {}).get('parent_room_id')
        return False
