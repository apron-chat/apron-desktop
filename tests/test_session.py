"""Integration: actual libsoup WebSocket client against local fictional server."""
import unittest
from gi.repository import GLib
from apron_gtk.session import Session


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.session = Session()
        self.ctx = GLib.MainContext.default()

    def tearDown(self):
        self.session.disconnect()

    def wait(self, predicate, seconds=5):
        import time
        end = time.monotonic() + seconds
        while not predicate() and time.monotonic() < end:
            while self.ctx.pending(): self.ctx.iteration(False)
            time.sleep(.005)
        self.assertTrue(predicate(), self.session.state + ': ' + self.session.error)

    def connect(self):
        self.session.connect('ws://127.0.0.1:18765', 'Test')
        self.wait(lambda: self.session.ready and 'general' in self.session.store.rooms)

    def test_auth_listing_history_pagination_send_reply_and_thread(self):
        self.connect()
        s = self.session
        self.assertEqual(s.token, 'local-fixture-token')
        pages = []
        s.history('general', callback=lambda *args: pages.append(args))
        self.wait(lambda: len(pages) == 1)
        self.assertIsNone(pages[0][1]); self.assertIsNotNone(pages[0][2])
        first = pages[0][0]['first_log_id']
        s.history('general', before=pages[0][2], callback=lambda *args: pages.append(args))
        self.wait(lambda: len(pages) == 2)
        self.assertLess(int(pages[1][0]['last_log_id']), int(first))
        replies = []
        s.send('general', 'Integration fixture reply', reply=str(1791536401080), mentions=['ada'], callback=lambda *a: replies.append(a))
        self.wait(lambda: bool(replies))
        mid = replies[0][0]['message_id']
        self.assertIn(mid, s.store.messages)  # broadcast precedes confirmation
        self.assertEqual(s.store.messages[mid]['reply_to']['message_id'], '1791536401080')
        created = []
        s.request('room_set', {'parent_room_id': 'general', 'title': 'Fixture thread'}, lambda *a: created.append(a))
        self.wait(lambda: bool(created))
        self.assertEqual(s.store.rooms[created[0][0]['room_id']]['parent_room_id'], 'general')

    def test_read_cursor_recovers_missed_mentions_and_is_idempotent(self):
        self.connect();s=self.session
        s.store.reads['general']=1791536401075
        preloaded=[];s.history('general',callback=lambda *a:preloaded.append(a))
        self.wait(lambda:bool(preloaded))  # cached historical snapshots must still recover unread
        pages=[]
        s.history('general',callback=lambda *a:pages.append(a),recover=True)
        self.wait(lambda:len(pages)==1)
        self.assertIsNone(pages[0][1])
        self.assertEqual(s.store.unread['general'],4)
        self.assertEqual(s.store.mentions['general'],1)
        s.history('general',callback=lambda *a:pages.append(a),recover=True)
        self.wait(lambda:len(pages)==2)
        self.assertEqual(s.store.unread['general'],4)
        self.assertEqual(s.store.mentions['general'],1)

    def test_cancelled_room_load_does_not_apply_or_call_back(self):
        self.connect();s=self.session;active=[True];answers=[]
        s.history('native',callback=lambda *a:answers.append(a),valid=lambda:active[0])
        active[0]=False
        self.wait(lambda:not s.pending)
        self.assertEqual(answers,[]);self.assertEqual(s.store.timeline('native'),[])

    def test_denied_token_stops_without_guest_fallback(self):
        s = self.session
        s.connect('ws://127.0.0.1:18765', token='deliberately-invalid-test-token')
        self.wait(lambda: not s.wanted)
        self.assertFalse(s.ready); self.assertEqual(s.retry_timer, 0)

    def test_send_error_preserves_error_and_no_automatic_retry(self):
        self.connect(); answers = []
        self.session.send('general', '/test-denied', callback=lambda *a: answers.append(a))
        self.wait(lambda: bool(answers))
        self.assertEqual(answers[0][1]['code'], -32001)
        self.assertTrue(self.session.ready)

    def test_reconnect_token_and_uncertain_send(self):
        self.connect(); answers = []
        self.session.send('general', '/test-disconnect', callback=lambda *a: answers.append(a))
        self.wait(lambda: bool(answers))
        self.assertIn('uncertain', answers[0][1]['message'])
        self.wait(lambda: self.session.ready, seconds=8)
        self.assertEqual(self.session.store.you['user_id'], 'demo_you')
        self.assertEqual(len(answers), 1)


if __name__ == '__main__': unittest.main()
