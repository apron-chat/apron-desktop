import json
import os
from pathlib import Path
import tempfile
import unittest
from apron_gtk.local_state import LocalState, StateWriter, account_key, MAX_BYTES, clean_account


class LocalStateTests(unittest.TestCase):
    def test_roundtrip_isolation_permissions_and_reset(self):
        with tempfile.TemporaryDirectory() as directory:
            s = LocalState(directory)
            a = account_key('wss://one.invalid/ws', 'alice')
            b = account_key('wss://one.invalid/ws', 'bob')
            c = account_key('wss://two.invalid/ws', 'alice')
            s.save(a, {'r': ('draft', '12', ['bob'])}, {'r': 15})
            self.assertEqual(s.restore(a)['drafts']['r'], ['draft','12',['bob']])
            self.assertEqual(s.restore(b)['drafts'], {})
            self.assertEqual(s.restore(c)['reads'], {})
            self.assertEqual(os.stat(s.path).st_mode & 0o777, 0o600)
            self.assertNotIn('one.invalid', s.path.read_text())
            s.reset(); self.assertFalse(s.path.exists())

    def test_limits_corruption_and_allowlist(self):
        with tempfile.TemporaryDirectory() as directory:
            s = LocalState(directory)
            for i in range(12):
                s.save(str(i).zfill(64), {str(j): ('x'*32000,None,[]) for j in range(300)}, {str(j): j for j in range(300)})
            self.assertLessEqual(s.path.stat().st_size, MAX_BYTES)
            self.assertLessEqual(len(s.load()), 8)
            self.assertLessEqual(len(s.restore(str(11).zfill(64))['reads']), 256)
            self.assertEqual(clean_account({'token':'secret','drafts':{'r':['x'*32001,None,[]]}}), {'drafts':{},'reads':{}})
            s.path.write_text('{bad'); self.assertEqual(s.load(), {})
            s.path.write_bytes(b'x'*(MAX_BYTES+1)); self.assertEqual(s.load(), {})

    def test_serialized_reset_wins(self):
        with tempfile.TemporaryDirectory() as directory:
            s=LocalState(directory); w=StateWriter(s)
            for i in range(50): w.submit('a'*64, {'r': (str(i),None,[])}, {})
            w.submit(reset=True); w.close()
            self.assertFalse(s.path.exists())

    def test_coalescing_preserves_distinct_accounts_and_latest_write(self):
        with tempfile.TemporaryDirectory() as directory:
            s=LocalState(directory); w=StateWriter(s)
            for i in range(8): w.submit(str(i).zfill(64), {'r': (str(i),None,[])}, {'r':i})
            w.submit('0'.zfill(64), {'r': ('latest',None,[])}, {'r':9})
            self.assertEqual(w.restore('0'.zfill(64))['drafts']['r'][0],'latest')
            w.close()
            self.assertEqual(len(s.load()),8)
            self.assertEqual(s.restore('0'.zfill(64))['drafts']['r'][0],'latest')
