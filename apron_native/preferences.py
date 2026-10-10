"""Opt-in non-secret preferences. Tokens and chat content never enter this file."""
import json
import os
from pathlib import Path
import tempfile
from .model import validate_url


class Preferences:
    def __init__(self, directory=None):
        base = directory or os.environ.get('APRON_NATIVE_STATE_DIR') or str(Path(os.environ.get('XDG_CONFIG_HOME', Path.home()/'.config')) / 'apron-native')
        self.path = Path(base)/'preferences.json'
        self.data = {'remember': False, 'theme': 'system', 'notifications': False}
        try:
            if self.path.stat().st_size <= 8192:
                self.data.update(self.clean(json.loads(self.path.read_text())))
        except (OSError, ValueError, TypeError):
            pass

    @staticmethod
    def clean(data):
        if not isinstance(data, dict): return {}
        out = {k: data[k] for k in ('remember', 'notifications', 'secure_session', 'local_state') if isinstance(data.get(k), bool)}
        if data.get('theme') in ('system','light','dark'): out['theme'] = data['theme']
        if out.get('remember'):
            try: out['url'] = validate_url(data.get('url', ''))
            except (ValueError, TypeError): pass
            if isinstance(data.get('name'), str): out['name'] = data['name'][:200]
        return out

    def save(self, **values):
        self.data = self.clean({**self.data, **values})
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        fd, temporary = tempfile.mkstemp(dir=self.path.parent, prefix='.preferences-')
        try:
            with os.fdopen(fd, 'w') as file:
                json.dump(self.data, file)
                file.flush(); os.fsync(file.fileno())
            os.replace(temporary, self.path)
        finally:
            Path(temporary).unlink(missing_ok=True)
