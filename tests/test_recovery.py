import unittest
from unittest.mock import patch
from apron_native.model import Store
from apron_native.recovery import scan, reconcile_page


def message(mid, log, room='r', **fields):
    return {'message_id':str(mid),'log_id':str(log),'room_id':room,'from':{'user_id':'other'},'body':{'text':'synthetic'},**fields}


def page(messages=(), first='10', last='20', more=False, lower='1', horizon='40'):
    return {'messages':list(messages), 'first_log_id':first,'last_log_id':last,'more':more,'history_log_id':lower,'latest_log_id':horizon}


class FakeSession:
    def __init__(self, replies): self.generation=1; self.store=Store(); self.replies=iter(replies); self.calls=[]
    def request(self, method, params, callback): self.calls.append(params); callback(next(self.replies), None)


class RecoveryTests(unittest.TestCase):
    def test_older_page_reconciles_edit_delete_and_move(self):
        base=page([message(10,10),message(11,11),message(12,12)])
        session=FakeSession([page([message(10,25,body={'text':'edited'}),message(11,26,deleted=True),message(12,27,'elsewhere')],first='25',last='27')])
        results=[]; reconcile_page(session,'r',base,lambda result,error:results.append((result,error)))
        result,error=results[0];self.assertIsNone(error)
        byid={m['message_id']:m for m in result['messages']}
        self.assertEqual(byid['10']['body']['text'],'edited');self.assertTrue(byid['11']['deleted'])
        self.assertEqual(byid['12']['room_id'],'elsewhere')
        self.assertEqual(session.calls[0]['after'],'21'); self.assertEqual(session.calls[0]['before'],'40')

    def test_inclusive_forward_cursors_and_fixed_horizon(self):
        s=FakeSession([page(first='21',last='30',more=True), page(first='31',last='40',horizon='99')])
        errors=[];scan(s,'r',21,40,lambda _:None,errors.append)
        self.assertEqual(errors,[None]);self.assertEqual([p['after'] for p in s.calls],['21','31'])
        self.assertEqual([p['before'] for p in s.calls],['40','40'])

    def test_gap_nonprogress_budget_and_cancel(self):
        for response in [page(lower='22'),page(last='20',more=True)]:
            s=FakeSession([response]);errors=[];scan(s,'r',21,40,lambda _:None,errors.append)
            self.assertIsNotNone(errors[0])
        with patch('apron_native.recovery.MAX_SCAN_PAGES',1):
            s=FakeSession([page(last='30',more=True)]);errors=[];scan(s,'r',21,40,lambda _:None,errors.append)
            self.assertIn('safety limit',errors[0]['message']);self.assertEqual(len(s.calls),1)
        s=FakeSession([]);scan(s,'r',21,40,lambda _:self.fail(),lambda _:self.fail(),valid=lambda:False)
        self.assertFalse(s.calls)

    def test_retention_invalidates_original_page_during_suffix_scan(self):
        s=FakeSession([page(first='25',last='30',lower='15')]);results=[]
        reconcile_page(s,'r',page([message(10,10)],first='10',last='20'),lambda *a:results.append(a))
        self.assertIsNone(results[0][0]);self.assertIn('retention',results[0][1]['message'])

    def test_newer_live_snapshot_wins_and_deleted_unread_clears(self):
        s=Store();s.you={'user_id':'me'};s.message(message(10,10),live=True)
        self.assertEqual(s.unread['r'],1)
        s.message(message(10,50,deleted=True));s.message(message(10,25))
        self.assertTrue(s.messages['10']['deleted']);self.assertEqual(s.unread['r'],0)
        s.discard_room_history('r');self.assertFalse(s.timeline('r'));self.assertEqual(s.cache_bytes,0)
