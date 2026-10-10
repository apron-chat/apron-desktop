"""Input-driven typing and idle reporting; no periodic network polling."""
import time
from gi.repository import GLib


class Attention:
    def __init__(self, session):
        self.session = session
        self.last_input = time.monotonic()
        self.idle = False
        self.idle_sent = False
        self.idle_pending = False
        self.legacy_idle = False
        self.retry_at = 0
        self.idle_timer = self.typing_timer = 0
        self.typing_room = None
        self.last_typing = 0
        self.closed = False

    def cancel_timer(self, field):
        if getattr(self,field): GLib.source_remove(getattr(self,field)); setattr(self,field,0)

    def ready(self, attended=True):
        self.idle_sent = False; self.idle_pending = False; self.legacy_idle = False; self.retry_at = 0
        self.typing_room = None; self.cancel_timer('typing_timer')
        self.cancel_timer('idle_timer')
        self.idle = not attended or time.monotonic()-self.last_input >= 300
        self.report_idle()
        self.idle_timer = GLib.timeout_add_seconds(300, self.check_idle)

    def input(self):
        if self.closed:return
        self.last_input=time.monotonic()
        if self.idle:
            self.idle=False
        self.report_idle()
        if not self.idle_timer:self.idle_timer=GLib.timeout_add_seconds(300,self.check_idle)

    def check_idle(self):
        self.idle_timer=0
        elapsed=time.monotonic()-self.last_input
        if elapsed>=300:
            self.idle=True;self.report_idle()
        else:self.idle_timer=GLib.timeout_add(max(1,int((300-elapsed)*1000)),self.check_idle)
        return False

    def report_idle(self):
        s=self.session
        if not s.ready or 'status' not in s.caps or self.idle_pending or self.idle==self.idle_sent or time.monotonic()<self.retry_at:return
        generation=s.generation;desired=self.idle;self.idle_pending=True
        def done(result,error):
            if generation!=s.generation:return
            self.idle_pending=False
            if error and error.get('code') == -32602 and desired and not self.legacy_idle:
                self.legacy_idle=True
                if s.error == error.get('message'): s.error=''
                self.report_idle();return
            if not error:
                self.idle_sent=desired
                if self.idle!=desired:self.report_idle()
            else:
                self.idle_sent=None;self.retry_at=time.monotonic()+30
            # Retry failures only on later input, never in a background loop.
        s.request('status',{'idle':(True if self.legacy_idle else max(0,int(time.monotonic()-self.last_input))) if desired else False},done)

    def typing(self,rid,has_text):
        self.input();s=self.session
        if self.typing_room and self.typing_room!=rid:self.stop_typing()
        if not has_text:self.stop_typing();return
        if not s.ready or 'activity' not in s.caps or not s.store.rooms.get(rid,{}).get('_joined'):return
        now=time.monotonic()
        if self.typing_room!=rid or now-self.last_typing>=12:
            s.notify('activity',{'room_id':rid,'typing':15});self.last_typing=now;self.typing_room=rid
        self.cancel_timer('typing_timer');self.typing_timer=GLib.timeout_add_seconds(5,self.stop_typing)

    def stop_typing(self):
        self.cancel_timer('typing_timer')
        if self.typing_room and self.session.ready and 'activity' in self.session.caps:
            self.session.notify('activity',{'room_id':self.typing_room,'typing':0})
        self.typing_room=None;return False

    def close(self):
        self.closed=True;self.stop_typing();self.cancel_timer('idle_timer')
