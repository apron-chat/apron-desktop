import threading
import os
import tempfile
import time
import unittest
from pathlib import Path
from http.server import ThreadingHTTPServer,BaseHTTPRequestHandler
from gi.repository import GLib,Gio
from apron_gtk.media import Transfers,transfer_url,file_title,MAX_TRANSFER

class Handler(BaseHTTPRequestHandler):
    seen=[]
    def log_message(self,*args):pass
    def do_GET(self):
        self.seen.append((self.path,dict(self.headers)))
        if self.path=='/redirect':
            self.send_response(302);self.send_header('Location','/secret');self.end_headers();return
        data=b'x'*(MAX_TRANSFER+1) if self.path=='/oversize' else b'synthetic attachment'
        self.send_response(200);self.end_headers()
        try:self.wfile.write(data)
        except (BrokenPipeError,ConnectionResetError):pass
    def do_PUT(self):
        data=self.rfile.read(int(self.headers['Content-Length']))
        self.seen.append((self.path,dict(self.headers)))
        self.send_response(201);self.end_headers();self.wfile.write(data)

class MediaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True);cls.thread.start()
        cls.origin=f'http://127.0.0.1:{cls.server.server_port}'
        cls.ws=cls.origin.replace('http:','ws:')+'/ws'
    @classmethod
    def tearDownClass(cls):cls.server.shutdown();cls.server.server_close();cls.thread.join()
    def wait(self,values):
        ctx=GLib.MainContext.default();end=time.monotonic()+5
        while not values and time.monotonic()<end:
            while ctx.pending():ctx.iteration(False)
            time.sleep(.002)
        self.assertTrue(values)
    def test_origin_and_scheme_policy(self):
        for url in ['https://other.invalid/a','http://chat.invalid/a','file:///etc/passwd','https://u:p@chat.invalid/a','https://chat.invalid:444/a','https://chat.invalid/a#x']:
            with self.assertRaises(ValueError):transfer_url(url,'wss://chat.invalid/ws')
        self.assertEqual(transfer_url('https://chat.invalid/a?opaque=x','wss://chat.invalid/ws'),'https://chat.invalid/a?opaque=x')
        self.assertEqual(file_title('../x\u202e.txt'),'x.txt')
    def test_get_put_have_no_auth_cookies_or_redirects(self):
        t=Transfers()
        for method in ['GET','PUT']:
            result=[];t.request(method,self.origin+'/file',self.ws,lambda *a:result.append(a),b'synthetic attachment' if method=='PUT' else None)
            self.wait(result);self.assertEqual(result[0],(b'synthetic attachment',None))
            headers={k.lower():v for k,v in Handler.seen[-1][1].items()}
            self.assertNotIn('authorization',headers);self.assertNotIn('cookie',headers);self.assertNotIn('referer',headers)
        result=[];t.request('GET',self.origin+'/redirect',self.ws,lambda *a:result.append(a));self.wait(result)
        self.assertIn('302',result[0][1]);self.assertFalse(any(path=='/secret' for path,_ in Handler.seen))
    def test_unknown_length_bound_cancel_and_local_file(self):
        t=Transfers();result=[];t.request('GET',self.origin+'/oversize',self.ws,lambda *a:result.append(a));self.wait(result)
        self.assertIn('8 MiB',result[0][1]);self.assertFalse(t.active)
        result=[];t.request('GET',self.origin+'/file',self.ws,lambda *a:result.append(a));t.cancel();self.wait(result)
        self.assertIsNotNone(result[0][1])
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'synthetic.txt';p.write_bytes(b'local synthetic')
            result=[];t.read_file(Gio.File.new_for_path(str(p)),lambda *a:result.append(a));self.wait(result)
            self.assertEqual(result[0],(b'local synthetic',None))
            fifo=Path(d)/'pipe';os.mkfifo(fifo)
            result=[];t.read_file(Gio.File.new_for_path(str(fifo)),lambda *a:result.append(a));self.wait(result)
            self.assertIn('regular file',result[0][1])
