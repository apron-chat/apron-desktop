import unittest
from unittest.mock import patch
from types import SimpleNamespace
from apron_gtk.presence import Presence
from apron_gtk.attention import Attention

class PresenceTests(unittest.TestCase):
    def test_expiry_stop_own_and_bounds(self):
        p=Presence()
        p.activity({'room_id':'r','from':{'user_id':'a'},'typing':8},'me',now=0)
        self.assertEqual(p.deadline(),8)
        p.expire(8);self.assertFalse(p.typing)
        for i in range(300):p.activity({'room_id':'r','from':{'user_id':str(i)},'typing':10000},'me',now=0)
        self.assertEqual(len(p.typing),256);self.assertEqual(p.deadline(),30)
        p.activity({'room_id':'r','from':{'user_id':'299'},'typing':0},'me',now=1)
        self.assertNotIn(('r','299'),p.typing)
        p.activity({'room_id':'r','from':{'user_id':'me'},'typing':8},'me',now=0)
        self.assertNotIn(('r','me'),p.typing)

    def test_muting_parent_global_timed_and_cycles(self):
        p=Presence();rooms={'thread':{'parent_room_id':'parent'},'parent':{'parent_room_id':'thread'}}
        p.status({'room_id':'parent','mute':15},now=0)
        self.assertTrue(p.muted('thread',rooms));p.expire(15);self.assertFalse(p.muted('thread',rooms))
        p.status({'mute':True});self.assertTrue(p.muted('any',rooms))
        p.status({'mute':False});self.assertFalse(p.muted('thread',rooms))

    def test_throttle_stop_and_idle_transition_requests(self):
        sent=[];requests=[]
        s=SimpleNamespace(ready=True,caps=['activity','status'],generation=1,store=SimpleNamespace(rooms={'r':{'_joined':True}}))
        s.notify=lambda m,p:sent.append((m,p))
        s.request=lambda m,p,cb:(requests.append((m,p)),cb({},None))
        a=Attention(s)
        try:
            a.ready(True)
            for _ in range(50):a.typing('r',True)
            self.assertEqual(len(sent),1);self.assertEqual(sent[0][1]['typing'],15)
            a.stop_typing();a.stop_typing();self.assertEqual(len(sent),2)
            a.cancel_timer('idle_timer');a.last_input-=301;a.check_idle()
            self.assertGreaterEqual(requests[-1][1]['idle'],300)
            a.input();self.assertIs(requests[-1][1]['idle'],False)
            s.caps=[];a.typing('r',True);self.assertEqual(len(sent),2)
        finally:a.close()
