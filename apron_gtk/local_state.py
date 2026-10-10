"""Opt-in bounded local drafts/read cursors. No credentials or message history."""
import hashlib
import json
import os
from pathlib import Path
import tempfile
from .model import number, validate_url

MAX_BYTES = 1024 * 1024
MAX_ACCOUNTS = 8
MAX_ROOMS = 256
MAX_DRAFT_BYTES = 32000


def account_key(url, user):
    validate_url(url)
    return hashlib.sha256((url + '\0' + user).encode()).hexdigest()


def clean_account(value):
    if not isinstance(value, dict): return {'drafts': {}, 'reads': {}}
    drafts, reads = {}, {}
    draft_bytes = 0
    for rid, draft in list(value.get('drafts', {}).items())[-MAX_ROOMS:][::-1] if isinstance(value.get('drafts'), dict) else []:
        if not isinstance(rid, str) or not 0 < len(rid) <= 256: continue
        if not isinstance(draft, (list, tuple)) or len(draft) != 3: continue
        text, reply, mentions = draft
        if not isinstance(text, str) or len(text.encode()) > MAX_DRAFT_BYTES: continue
        if reply is not None:
            try: number(reply)
            except (TypeError, ValueError): reply = None
        mentions = [x for x in mentions[:64] if isinstance(x, str) and len(x) <= 256] if isinstance(mentions, list) else []
        if text or reply:
            size = len(json.dumps([rid, text, reply, mentions]).encode())
            if draft_bytes + size <= 768 * 1024:
                drafts[rid] = [text, reply, mentions]; draft_bytes += size
    for rid, mid in list(value.get('reads', {}).items())[-MAX_ROOMS:] if isinstance(value.get('reads'), dict) else []:
        if not isinstance(rid, str) or not 0 < len(rid) <= 256: continue
        try: reads[rid] = number(str(mid))
        except (TypeError, ValueError): pass
    return {'drafts': dict(reversed(list(drafts.items()))), 'reads': reads}


class LocalState:
    def __init__(self, directory):
        self.path = Path(directory) / 'local-state.json'

    def load(self):
        try:
            with self.path.open('rb') as f: raw = f.read(MAX_BYTES + 1)
            if len(raw) > MAX_BYTES: return {}
            data = json.loads(raw)
            if not isinstance(data, dict) or data.get('version') != 1: return {}
            accounts = data.get('accounts', {})
            if not isinstance(accounts, dict): return {}
            return {k: clean_account(v) for k, v in list(accounts.items())[-MAX_ACCOUNTS:] if isinstance(k, str) and len(k) == 64}
        except (OSError, ValueError, TypeError): return {}

    def restore(self, key):
        return self.load().get(key, {'drafts': {}, 'reads': {}})

    def save(self, key, drafts, reads):
        accounts = self.load()
        accounts.pop(key, None)
        accounts[key] = clean_account({'drafts': drafts, 'reads': reads})
        while len(accounts) > MAX_ACCOUNTS: accounts.pop(next(iter(accounts)))
        def encoded(): return json.dumps({'version': 1, 'accounts': accounts}, ensure_ascii=False).encode()
        raw = encoded()
        while len(raw) > MAX_BYTES:
            oldest = next(iter(accounts))
            if oldest != key: accounts.pop(oldest)
            elif accounts[key]['drafts']: accounts[key]['drafts'].pop(next(iter(accounts[key]['drafts'])))
            else: break
            raw = encoded()
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        fd, tmp = tempfile.mkstemp(dir=self.path.parent, prefix='.local-state-')
        try:
            with os.fdopen(fd, 'wb') as f: f.write(raw); f.flush(); os.fsync(f.fileno())
            os.replace(tmp, self.path)
            self._sync_directory()
        finally: Path(tmp).unlink(missing_ok=True)

    def _sync_directory(self):
        fd = os.open(self.path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try: os.fsync(fd)
        finally: os.close(fd)

    def reset(self):
        self.path.unlink(missing_ok=True)
        if self.path.parent.exists(): self._sync_directory()


class StateWriter:
    """One active write plus at most eight coalesced account writes; never a polling loop."""
    def __init__(self, state, failed=lambda: None):
        import threading
        from concurrent.futures import ThreadPoolExecutor
        self.state, self.failed = state, failed
        self.lock = threading.Lock()
        self.executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix='apron-state')
        self.pending = {}
        self.snapshots = {}
        self.reset_pending = False
        self.reset_active = False
        self.future = None

    def submit(self, key=None, drafts=None, reads=None, reset=False):
        operation = None if reset else (key, clean_account({'drafts': drafts, 'reads': reads}))
        with self.lock:
            if reset:
                self.pending.clear(); self.snapshots.clear(); self.reset_pending = True
            else:
                self.pending.pop(key, None); self.pending[key] = operation
                self.snapshots.pop(key, None); self.snapshots[key] = operation[1]
                while len(self.snapshots) > MAX_ACCOUNTS: self.snapshots.pop(next(iter(self.snapshots)))
                while len(self.pending) > MAX_ACCOUNTS: self.pending.pop(next(iter(self.pending)))
            if self.future is None or self.future.done(): self.future = self.executor.submit(self._drain)

    def restore(self, key):
        from copy import deepcopy
        with self.lock:
            if key in self.snapshots: return deepcopy(self.snapshots[key])
            if self.reset_pending or self.reset_active: return {'drafts': {}, 'reads': {}}
        return self.state.restore(key)

    def _drain(self):
        while True:
            with self.lock:
                if not self.pending and not self.reset_pending:
                    self.future = None
                    return
                reset = self.reset_pending
                if reset: self.reset_pending = False; self.reset_active = True
                else: operation = self.pending.pop(next(iter(self.pending)))
            try:
                if reset: self.state.reset()
                else:
                    key, data = operation
                    self.state.save(key, data['drafts'], data['reads'])
            except OSError: self.failed()
            finally:
                if reset:
                    with self.lock: self.reset_active = False

    def close(self):
        self.executor.shutdown(wait=True)
