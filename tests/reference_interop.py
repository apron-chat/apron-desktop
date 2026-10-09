"""Actual native libsoup session against the official Go server on loopback.
Run an isolated --store memory server at 127.0.0.1:18766 first.
"""
import time
import unittest
from gi.repository import GLib
from apron_native.session import Session


class OfficialGoInterop(unittest.TestCase):
    def setUp(self):
        self.s = Session()
        self.s.connect('ws://127.0.0.1:18766/ws', 'Native integration test')
        self.wait(lambda: self.s.ready and 'general' in self.s.store.rooms)

    def tearDown(self):
        self.s.disconnect()

    def wait(self, predicate, seconds=10):
        ctx = GLib.MainContext.default()
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            while ctx.pending(): ctx.iteration(False)
            if predicate(): return
            time.sleep(.002)
        self.fail(self.s.state + ': ' + self.s.error)

    def request(self, method, params):
        answer = []
        self.s.request(method, params, lambda *args: answer.append(args))
        self.wait(lambda: bool(answer))
        self.assertIsNone(answer[0][1], answer[0][1])
        return answer[0][0]

    def test_connect_list_history_send_reply_thread_and_reconnect(self):
        s = self.s
        self.assertEqual(s.server['agent'], 'apron-go/8')
        self.assertIn('history', s.caps)
        listing = self.request('room_list', {'filter': 'joined', 'members': True})
        self.assertTrue(any(r['room_id'] == 'general' for r in listing['joined']))
        first = self.request('message', {'room_id': 'general', 'body': {'text': 'Synthetic native interop message'}})
        mid = first['message_id']
        self.assertIn(mid, s.store.messages)
        reply = self.request('message', {'room_id': 'general', 'reply_to': {'message_id': mid},
                                        'body': {'text': 'Synthetic native interop reply', 'mentions': [s.store.you['user_id']]}})
        self.assertEqual(s.store.messages[reply['message_id']]['reply_to']['message_id'], mid)
        thread = self.request('room_set', {'parent_room_id': 'general', 'title': 'Synthetic native interop thread'})
        rid = thread['room_id']
        self.assertEqual(s.store.rooms[rid]['parent_room_id'], 'general')
        self.request('message', {'room_id': rid, 'body': {'text': 'Synthetic thread message'}})
        pages = []
        s.history(rid, callback=lambda *a: pages.append(a))
        self.wait(lambda: bool(pages))
        self.assertIsNone(pages[0][1])
        self.assertTrue(s.store.timeline(rid))
        # Enough records to require a second backward page.
        for i in range(65):
            self.request('message', {'room_id': 'general', 'body': {'text': f'Synthetic pagination record {i}'}})
        pages = []
        s.history('general', callback=lambda *a: pages.append(a))
        self.wait(lambda: len(pages) == 1)
        self.assertIsNotNone(pages[0][2])
        s.history('general', pages[0][2], callback=lambda *a: pages.append(a))
        self.wait(lambda: len(pages) == 2)
        self.assertLess(int(pages[1][0]['last_log_id']), int(pages[0][0]['first_log_id']))
        generation = s.generation
        s.ws.close(1000, 'Synthetic reconnect test')
        self.wait(lambda: s.generation > generation and s.ready)
        self.wait(lambda: 'general' in s.store.rooms)
        # Reference guest policy may assign a new identity; do not assert token resume.
        pages = []
        s.history('general', callback=lambda *a: pages.append(a))
        self.wait(lambda: bool(pages))
        self.assertIsNone(pages[0][1])
        self.assertTrue(any('Synthetic pagination' in m.get('body', {}).get('text', '') for m in s.store.timeline('general')))


if __name__ == '__main__': unittest.main(verbosity=2)
