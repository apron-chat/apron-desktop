import unittest
from unittest.mock import patch
from apron_native.model import Store, number, validate_url, safe_text
from apron_native.render import markdown


def msg(mid='10', log='10', room='general', **fields):
    return {'message_id': mid, 'log_id': log, 'room_id': room, 'from': {'user_id': 'ada'}, 'body': {'text': 'hello'}, **fields}


class ModelTests(unittest.TestCase):
    def test_numeric_ids_are_not_lexicographic(self):
        s = Store(); s.message(msg('9', '9')); s.message(msg())
        self.assertEqual([m['message_id'] for m in s.timeline('general')], ['9', '10'])
        self.assertGreater(number('99999999999999999999'), number('10000000000000000000'))

    def test_snapshot_replay_edit_move_delete(self):
        s = Store(); s.message(msg())
        s.message(msg(log='30', room='thread', deleted=True, body={}))
        s.message(msg(log='20'))
        self.assertEqual(s.timeline('general'), [])
        self.assertTrue(s.timeline('thread')[0]['deleted'])
        self.assertFalse(s.message(msg(log='30')))

    def test_embedded_snapshot(self):
        s = Store(); s.message(msg('30', '30', reply_to=msg(log='20')))
        self.assertEqual(s.messages['10']['log_id'], '20')

    def test_mentions_are_explicit_and_history_does_not_notify(self):
        s = Store(); s.you = {'user_id': 'me'}
        s.message(msg(body={'text': '@me'}), live=True)
        self.assertEqual(s.unread['general'], 1)
        self.assertNotIn('general', s.mentions)
        s.message(msg('11', '11', body={'text': 'Hello', 'mentions': ['me']}), live=True)
        self.assertEqual(s.mentions['general'], 1)
        s.message(msg('12', '12', body={'mentions': ['me']}))
        self.assertEqual(s.unread['general'], 2)
        s.mark_read('general', '12')
        self.assertEqual(s.unread['general'], 0)

    def test_pagination_uses_log_bounds_not_compacted_messages(self):
        s = Store()
        cursor = s.history('general', {'messages': [msg('10', '18')], 'first_log_id': '12', 'last_log_id': '25',
                                      'history_log_id': '1', 'latest_log_id': '30', 'more': True})
        self.assertEqual(cursor, '11')

    def test_retention_removes_unverifiable_cache(self):
        s = Store(); s.message(msg())
        s.history('general', {'history_log_id': '20', 'latest_log_id': '30', 'more': False})
        self.assertEqual(s.timeline('general'), [])
        s.message(msg('25', '25'))
        s.history('general', {'history_log_id': None, 'latest_log_id': '30', 'more': False})
        self.assertEqual(s.timeline('general'), [])

    def test_bounded_memory(self):
        with patch('apron_native.model.MAX_MESSAGES', 4), patch('apron_native.model.MAX_ROOMS', 3):
            s = Store()
            for i in range(20):
                s.message(msg(str(i), str(i), str(i)))
            self.assertLessEqual(len(s.messages), 4)
            self.assertLessEqual(len(s.rooms), 3)
            self.assertTrue(s.evicted)

    def test_byte_budget_and_partial_read_cursor(self):
        with patch('apron_native.model.MAX_CACHE_BYTES', 700):
            s = Store(); s.you = {'user_id': 'me'}
            for i in range(10, 20):
                s.message(msg(str(i), str(i), body={'text': 'x' * 200}), live=True)
            self.assertLessEqual(s.cache_bytes, 700)
            self.assertTrue(s.evicted)
            s.mark_read('general', '15')
            self.assertEqual(s.unread['general'], 4)
            s.mark_read('general', '12')
            self.assertEqual(s.unread['general'], 4)

    def test_safe_url(self):
        for url in ['https://example.com', 'ws://example.com', 'wss://user:secret@example.com', 'wss://example.com?token=x', 'wss://example.com:bad', 'wss://bad host/ws']:
            with self.assertRaises(ValueError): validate_url(url)
        self.assertEqual(validate_url('ws://127.0.0.1:18765'), 'ws://127.0.0.1:18765')
        self.assertEqual(validate_url('wss://example.com/ws'), 'wss://example.com/ws')

    def test_markdown_never_passes_untrusted_markup(self):
        result = markdown('**bold** <script>alert(1)</script> ![picture](https://tracking.invalid/i) [x](javascript:bad)')
        self.assertIn('<b>bold</b>', result)
        self.assertNotIn('<script>', result)
        self.assertNotIn('<a ', result)
        self.assertIn('&lt;script&gt;', result)
        self.assertNotIn('tracking.invalid', result)
        self.assertEqual(safe_text('a\x00b\u202ec'), 'abc')

    def test_current_user_merge_and_room_replay(self):
        s = Store(); s.user({'user_id': 'a', 'name': 'Ada'}); s.user({'user_id': 'a', 'status': 'idle'})
        self.assertEqual(s.users['a']['name'], 'Ada')
        s.room({'room_id': 'r', 'log_id': '20', 'title': 'New'}, True)
        s.room({'room_id': 'r', 'log_id': '10', 'title': 'Old'})
        self.assertEqual(s.rooms['r']['title'], 'New')


if __name__ == '__main__': unittest.main()
