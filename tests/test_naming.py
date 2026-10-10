import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from apron_gtk.preferences import Preferences

class NamingTests(unittest.TestCase):
    def test_existing_legacy_state_directory_is_reused(self):
        with tempfile.TemporaryDirectory() as directory,patch.dict(os.environ,{'XDG_CONFIG_HOME':directory}):
            for key in ['APRON_GTK_STATE_DIR','APRON_NATIVE_STATE_DIR']:os.environ.pop(key,None)
            legacy=Path(directory)/'apron-native';legacy.mkdir()
            self.assertEqual(Preferences().path.parent,legacy)
            modern=Path(directory)/'apron-gtk';modern.mkdir()
            self.assertEqual(Preferences().path.parent,modern)
    def test_new_override_wins_and_old_override_still_works(self):
        with tempfile.TemporaryDirectory() as directory,patch.dict(os.environ,{'APRON_NATIVE_STATE_DIR':directory}):
            os.environ.pop('APRON_GTK_STATE_DIR',None)
            self.assertEqual(Preferences().path.parent,Path(directory))
            os.environ['APRON_GTK_STATE_DIR']=directory+'/new'
            self.assertEqual(Preferences().path.parent,Path(directory)/'new')
