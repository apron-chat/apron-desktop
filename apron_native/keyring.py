"""Optional async Secret Service integration. Never falls back to a file."""
try:
    import gi
    gi.require_version('Secret', '1')
    from gi.repository import Secret
except (ImportError, ValueError):
    Secret = None


class Keyring:
    def __init__(self):
        self.available = Secret is not None
        self.schema = Secret.Schema.new('chat.apron.Native', Secret.SchemaFlags.NONE,
                                        {'server':Secret.SchemaAttributeType.STRING}) if self.available else None

    def unavailable(self, callback):
        callback(None, 'System Secret Service bindings are unavailable. Tokens remain in memory.')

    def store(self, server, token, callback):
        if not self.available: return self.unavailable(callback)
        def done(source, result, *_):
            try: callback(bool(Secret.password_store_finish(result)), None)
            except Exception: callback(None, 'Could not save the session in the system keyring. No file fallback was used.')
        Secret.password_store(self.schema, {'server':server}, Secret.COLLECTION_DEFAULT,
                              'Apron session · ' + server, token, None, done)

    def lookup(self, server, callback):
        if not self.available: return self.unavailable(callback)
        def done(source, result, *_):
            try: callback(Secret.password_lookup_finish(result), None)
            except Exception: callback(None, 'Could not read the system keyring. Unlock or configure a Secret Service provider.')
        Secret.password_lookup(self.schema, {'server':server}, None, done)

    def forget(self, server, callback):
        if not self.available: return self.unavailable(callback)
        def done(source, result, *_):
            try: callback(bool(Secret.password_clear_finish(result)), None)
            except Exception: callback(None, 'Could not remove the stored session from the system keyring.')
        Secret.password_clear(self.schema, {'server':server}, None, done)
