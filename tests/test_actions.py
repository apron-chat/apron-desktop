import json
import os
import tempfile
import unittest
from pathlib import Path
from apron_gtk.actions import message_save, same_snapshot
from apron_gtk.model import Store
from apron_gtk.preferences import Preferences


class DailyActionTests(unittest.TestCase):
    def message(self):
        return {'message_id':'10','log_id':'12','room_id':'r','from':{'user_id':'me'},
                'body':{'text':'Before','format':'markdown','mentions':['a'],'embeds':[{'kind':'link','url':'https://example.invalid'}]},
                'reply_to':{'message_id':'8','body':{'text':'Reference'}},'ext':{'x-custom':{'value':1}}}

    def test_edit_move_preserve_all_client_fields(self):
        msg = self.message(); saved = message_save(msg, text='After', room_id='thread')
        self.assertEqual(saved['body']['embeds'], msg['body']['embeds'])
        self.assertEqual(saved['body']['mentions'], ['a'])
        self.assertEqual(saved['reply_to'], {'message_id':'8'})
        self.assertEqual(saved['ext'], msg['ext'])
        self.assertNotIn('from', saved); self.assertNotIn('log_id', saved)
        self.assertEqual(msg['body']['text'], 'Before')

    def test_delete_minimal_and_stale_action_conflict(self):
        msg = self.message(); s = Store(); s.message(msg)
        self.assertTrue(same_snapshot(s, msg))
        s.message({**msg,'log_id':'13'})
        self.assertFalse(same_snapshot(s, msg))
        self.assertEqual(message_save(msg, deleted=True), {'message_id':'10','room_id':'r','deleted':True})

    def test_reaction_replay_per_user_and_empty_set(self):
        s = Store()
        def record(log, user, emojis):
            return {'message_id':'10','room_id':'r','log_id':str(log),'reactions':[{'from':{'user_id':user},'emojis':emojis}]}
        s.reaction(record(10,'a',['👍','👍'])); s.reaction(record(11,'b',['👍']))
        self.assertEqual(s.reaction_counts('10'), {'👍':2})
        s.reaction(record(12,'a',[])); s.reaction(record(9,'a',['❤️']))
        self.assertEqual(s.reaction_counts('10'), {'👍':1})

    def test_membership_replay_and_room_edit_keep_member_snapshot(self):
        s = Store(); s.room({'room_id':'r','log_id':'1','members':[{'user_id':'a'}]},True)
        s.room({'room_id':'r','log_id':'2','title':'Changed'})
        self.assertEqual(s.rooms['r']['members'], [{'user_id':'a'}])
        s.membership({'room_id':'r','log_id':'4','members':[{'user':{'user_id':'a'},'joined':False}]})
        s.membership({'room_id':'r','log_id':'3','members':[{'user':{'user_id':'a'},'joined':True}]})
        self.assertEqual(s.rooms['r']['members'], [])

    def test_preferences_never_store_tokens_or_chat_and_opt_out_clears_address(self):
        with tempfile.TemporaryDirectory() as d:
            p = Preferences(d)
            p.save(remember=True,url='wss://example.invalid/ws',name='Synthetic',theme='dark',token='DO-NOT-PERSIST',draft='PRIVATE')
            text=p.path.read_text(); self.assertNotIn('DO-NOT',text); self.assertNotIn('PRIVATE',text)
            self.assertEqual(os.stat(p.path).st_mode & 0o777, 0o600)
            self.assertEqual(Preferences(d).data['url'], 'wss://example.invalid/ws')
            p.save(remember=False)
            self.assertNotIn('url', Preferences(d).data)
            self.assertNotIn('name', Preferences(d).data)

    def test_complete_user_replaces_cleared_fields(self):
        s=Store();s.user({'user_id':'a','name':'Ada','status':'online'})
        s.user({'user_id':'a'},complete=True)
        self.assertNotIn('name',s.users['a']);self.assertNotIn('status',s.users['a'])

    def test_keyring_unavailable_has_no_file_fallback(self):
        from unittest.mock import patch
        from apron_gtk.keyring import Keyring
        with patch('apron_gtk.keyring.Secret', None):
            keyring=Keyring();results=[]
            keyring.store('wss://example.invalid', 'Synthetic-only', lambda *a:results.append(a))
            self.assertFalse(keyring.available)
            self.assertIsNone(results[0][0]);self.assertIn('remain in memory',results[0][1])

    def test_keyring_async_adapter_calls_only_secret_service(self):
        from unittest.mock import patch, MagicMock
        from apron_gtk.keyring import Keyring
        secret=MagicMock()
        secret.password_store.side_effect=lambda *args: args[-1](None, 'result')
        secret.password_store_finish.return_value=True
        secret.password_lookup.side_effect=lambda *args: args[-1](None, 'result')
        secret.password_lookup_finish.return_value='synthetic-token'
        secret.password_clear.side_effect=lambda *args: args[-1](None, 'result')
        secret.password_clear_finish.return_value=True
        with patch('apron_gtk.keyring.Secret', secret):
            keyring=Keyring();results=[]
            keyring.store('wss://example.invalid', 'synthetic-token', lambda *a:results.append(a))
            keyring.lookup('wss://example.invalid',lambda *a:results.append(a))
            keyring.forget('wss://example.invalid',lambda *a:results.append(a))
            self.assertEqual(results,[(True,None),('synthetic-token',None),(True,None)])

    def test_redaction_can_replace_content_at_same_log_position(self):
        s=Store();msg=self.message();s.message(msg)
        self.assertTrue(s.message({k:v for k,v in {**msg,'deleted':True}.items() if k not in ('body','ext')}))
        self.assertNotIn('body', s.messages['10'])
        self.assertFalse(s.message(msg))

    def test_serialized_keyring_rotation_then_forget(self):
        import gi
        gi.require_version('Gtk','4.0');gi.require_version('Adw','1')
        from apron_gtk.ui_actions import DailyActions
        from types import SimpleNamespace
        class Harness(DailyActions):
            def queue_render(self): pass
        with tempfile.TemporaryDirectory() as d:
            h=Harness();h.preferences=Preferences(d);h.preferences.data['secure_session']=True
            h.session=SimpleNamespace(url='wss://example.invalid',token='old-synthetic',generation=1)
            h.keyring_jobs={};h.keyring_busy=False;calls=[]
            h.keyring=SimpleNamespace(store=lambda url,token,cb:calls.append(('store',token,cb)), forget=lambda url,cb:calls.append(('forget',None,cb)))
            h.save_secure_session();h.session.token='new-synthetic';h.save_secure_session()
            self.assertEqual(len(calls),1)
            calls[0][2](True,None)
            self.assertEqual(calls[1][1],'new-synthetic')
            done=[];h.forget_secure_session(h.session.url,lambda *a:done.append(a))
            self.assertEqual(len(calls),2)
            calls[1][2](True,None)
            self.assertEqual(calls[2][0],'forget')
            calls[2][2](True,None)
            self.assertEqual(done,[(True,None)])
            self.assertFalse(h.preferences.data['secure_session'])

if __name__ == '__main__': unittest.main()
