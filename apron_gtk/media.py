"""Explicit bounded transfers. Isolated HTTP session: no auth, cookies or redirects."""
from urllib.parse import urlsplit
import gi
gi.require_version('Soup','3.0')
from gi.repository import Soup, Gio, GLib
from .model import safe_text, validate_url

MAX_TRANSFER = 8 * 1024 * 1024
MAX_ATTACHMENTS = 4


def transfer_url(url, server):
    validate_url(server)
    if not isinstance(url,str) or len(url)>8192 or any(c.isspace() or ord(c)<32 for c in url):
        raise ValueError('Invalid attachment URL.')
    target, source = urlsplit(url), urlsplit(server)
    scheme = 'https' if source.scheme=='wss' else 'http'
    if target.scheme!=scheme or target.hostname!=source.hostname or (target.port or (443 if scheme=='https' else 80))!=(source.port or (443 if scheme=='https' else 80)):
        raise ValueError('Only attachments hosted by this server can be transferred. External media stays unloaded.')
    if target.username or target.password or target.fragment:
        raise ValueError('Attachment URLs cannot contain user credentials or fragments.')
    return url


def file_title(name):
    name = name if isinstance(name,str) else 'attachment'
    return safe_text(name.replace('\\','/').split('/')[-1],180) or 'attachment'


def validate_attachments(files):
    if not 1 <= len(files) <= MAX_ATTACHMENTS:
        raise ValueError('Choose between one and four attachments.')
    if any(not isinstance(data, bytes) for _,data in files) or sum(len(data) for _,data in files)>MAX_TRANSFER:
        raise ValueError('Attachments must total no more than 8 MiB.')
    return [(file_title(title),data) for title,data in files]


class Transfers:
    def __init__(self):
        self.http=Soup.Session(timeout=30)
        self.active=False
        self.cancellable=None

    def cancel(self):
        if self.cancellable:self.cancellable.cancel()

    def _begin(self,callback):
        if self.active:raise ValueError('Finish or cancel the current transfer first.')
        self.active=True;self.cancellable=Gio.Cancellable();cancel=self.cancellable
        completed=False
        def expired():
            nonlocal timeout
            timeout=0;cancel.cancel();return False
        timeout=GLib.timeout_add_seconds(120,expired)
        def done(value=None,error=None):
            nonlocal completed
            if completed:return
            if timeout:GLib.source_remove(timeout)
            completed=True;self.active=False;self.cancellable=None
            callback(value,error)
        return cancel,done

    def _read(self,stream,cancel,done):
        chunks=bytearray()
        def read():stream.read_bytes_async(min(65536,MAX_TRANSFER-len(chunks)+1),GLib.PRIORITY_DEFAULT,cancel,received)
        def received(source,result):
            try:
                data=source.read_bytes_finish(result).get_data()
                if not data:
                    source.close(None);done(bytes(chunks),None);return
                chunks.extend(data)
                if len(chunks)>MAX_TRANSFER:raise ValueError('Attachment exceeds the 8 MiB client limit.')
                read()
            except (GLib.Error,ValueError) as error:
                try:source.close(None)
                except GLib.Error:pass
                done(None,str(error) if isinstance(error,ValueError) else 'Transfer cancelled or interrupted.')
        read()

    def read_file(self,file,callback):
        if not file.get_path():raise ValueError('Choose a local file.')
        cancel,done=self._begin(callback)
        # Open nonblocking before fstat: choosing a FIFO/device cannot hang a GIO worker.
        import threading, os, stat
        def read_local():
            try:
                fd=os.open(file.get_path(),os.O_RDONLY | os.O_NONBLOCK)
                with os.fdopen(fd,'rb') as source:
                    info=os.fstat(source.fileno())
                    if not stat.S_ISREG(info.st_mode) or info.st_size>MAX_TRANSFER:
                        GLib.idle_add(lambda:(done(None,'Choose a regular file no larger than 8 MiB.'),False)[1]);return
                    data=source.read(MAX_TRANSFER+1)
                if cancel.is_cancelled():error='File selection cancelled.'
                elif len(data)>MAX_TRANSFER:error='Attachment exceeds the 8 MiB client limit.'
                else:error=None
                GLib.idle_add(lambda:(done(None if error else data,error),False)[1])
            except OSError:
                GLib.idle_add(lambda:(done(None,'Could not read this local file.'),False)[1])
        threading.Thread(target=read_local,name='apron-file-read',daemon=True).start()

    def write_file(self,file,data,callback):
        if not file.get_path() or len(data)>MAX_TRANSFER:raise ValueError('Choose a local destination for a file no larger than 8 MiB.')
        cancel,done=self._begin(callback)
        def saved(source,result):
            try:source.replace_contents_finish(result);done(True,None)
            except GLib.Error:done(None,'Saving cancelled or failed. Check the destination file.')
        file.replace_contents_async(data,None,False,Gio.FileCreateFlags.PRIVATE,cancel,saved)

    def request(self,method,url,server,callback,data=None):
        url=transfer_url(url,server)
        if method not in ('GET','PUT'):raise ValueError('Unsupported transfer method.')
        if data is not None and len(data)>MAX_TRANSFER:raise ValueError('Attachment exceeds the 8 MiB client limit.')
        cancel,done=self._begin(callback)
        msg=Soup.Message.new(method,url);msg.set_flags(Soup.MessageFlags.NO_REDIRECT)
        if data is not None:msg.set_request_body_from_bytes('application/octet-stream',GLib.Bytes.new(data))
        def response(http,result):
            try:
                stream=http.send_finish(result)
                status=int(msg.get_status())
                if not 200<=status<300:
                    stream.close(None)
                    detail={413:'The server rejected the file size.',415:'The server rejected this file type.',409:'This single-use upload URL was already used.'}.get(status,f'Transfer failed (HTTP {status}). Redirects are not followed.')
                    done(None,detail);return
                if msg.get_response_headers().get_content_length()>MAX_TRANSFER:
                    stream.close(None);done(None,'Attachment exceeds the 8 MiB client limit.');return
                self._read(stream,cancel,done)
            except GLib.Error:done(None,'Transfer cancelled or interrupted. Upload delivery may be uncertain; check the message before retrying.')
        self.http.send_async(msg,GLib.PRIORITY_DEFAULT,cancel,response)


class MediaActions:
    def transfer_state(self,text=''):
        self.transfer_note.set_text(safe_text(text,500))
        self.transfer_note.set_visible(bool(text))
        self.transfer_cancel.set_visible(self.transfers.active)
        self.update_send()

    def choose_attachment(self):
        from .app import Gtk
        if self.picking_file or self.attachment_dialog or self.transfers.active:return
        if not self.session.ready or 'embed:upload' not in self.session.caps:return
        self.picking_file=True;generation=self.session.generation;rid=self.room_id
        chooser=Gtk.FileDialog(title='Attach up to four files · 8 MiB total')
        def chosen(dialog,result):
            try:selected=dialog.open_multiple_finish(result)
            except GLib.Error:self.picking_file=False;return
            if self.closing or generation!=self.session.generation or rid!=self.room_id:
                self.picking_file=False;return
            if not 1<=selected.get_n_items()<=MAX_ATTACHMENTS:
                self.picking_file=False;self.transfer_state('Choose between one and four files.');return
            files=[]
            def next_file(index=0):
                if self.closing or generation!=self.session.generation or rid!=self.room_id:
                    self.picking_file=False;files.clear();return
                if index==selected.get_n_items():
                    self.picking_file=False;self.transfer_state('')
                    self.confirm_attachments(files,rid,generation);return
                file=selected.get_item(index)
                def loaded(data,error):
                    if self.closing:
                        self.picking_file=False;files.clear();return
                    if error:
                        self.picking_file=False;files.clear()
                        if not self.closing:self.transfer_state(error)
                        return
                    files.append((file_title(file.get_basename()),data))
                    if sum(len(value) for _,value in files)>MAX_TRANSFER:
                        self.picking_file=False;files.clear();self.transfer_state('Attachments must total no more than 8 MiB.');return
                    next_file(index+1)
                try:
                    self.transfers.read_file(file,loaded)
                    self.transfer_state(f'Reading file {index+1} of {selected.get_n_items()}…')
                except ValueError as error:
                    self.picking_file=False;files.clear();self.transfer_state(str(error))
            next_file()
        chooser.open_multiple(self,None,chosen)

    def confirm_attachment(self,title,data,rid,generation):
        return self.confirm_attachments([(title,data)],rid,generation)

    def confirm_attachments(self,files,rid,generation):
        from .app import Gtk, label, button, box
        if self.attachment_dialog:return
        files=validate_attachments(files)
        original=(self.text(),self.reply,self.mention_ids[:])
        win,root=self.dialog('Send attachments');self.attachment_dialog=win
        dismissed=[False];pending=[False]
        def closing(*_):
            dismissed[0]=True;self.attachment_dialog=None
            if not pending[0]:files.clear()
            return False
        win.connect('close-request',closing)
        for title,data in files:root.append(label(f'{title} · {len(data):,} bytes','author',True))
        root.append(label('One message, uploaded in order after confirmation. Failed or cancelled files are not retried automatically.','dim-label',True))
        caption=Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        caption.get_buffer().set_text(original[0]);caption.set_size_request(-1,80)
        caption.update_property([Gtk.AccessibleProperty.LABEL],['Attachment message'])
        scroll=Gtk.ScrolledWindow(min_content_height=80,max_content_height=150,propagate_natural_height=True)
        scroll.set_child(caption);root.append(scroll)
        reply_toggle=None
        if original[1]:
            target=self.session.store.messages.get(original[1],{})
            excerpt=safe_text(target.get('body',{}).get('text') or original[1],100)
            reply_toggle=Gtk.CheckButton(label='Reply to: '+excerpt,active=True)
            child=reply_toggle.get_child()
            if isinstance(child,Gtk.Label):child.set_wrap(True)
            root.append(reply_toggle)
        if original[2]:root.append(label('Mentions: '+', '.join('@'+safe_text(uid,40) for uid in original[2]),'metadata',True))
        error_label=label('','warning',True);root.append(error_label)
        def submit():
            if pending[0]:return
            if not self.session.ready or generation!=self.session.generation or rid!=self.room_id or 'embed:upload' not in self.session.caps:
                error_label.set_text('Connection changed. Close and choose the files again.');return
            if self.transfers.active:
                error_label.set_text('Finish or cancel the current transfer first.');return
            if not self.session.store.rooms.get(rid,{}).get('_joined'):
                error_label.set_text('Join the room before attaching files.');return
            buffer=caption.get_buffer();text=buffer.get_text(buffer.get_start_iter(),buffer.get_end_iter(),False)
            if len(text.encode())>32000:error_label.set_text('Attachment message exceeds 32,000 bytes.');return
            pending[0]=True;control.set_sensitive(False)
            params={'room_id':rid,'body':{'text':text,'format':'plain','mentions':original[2],
                    'embeds':[{'kind':'upload','title':title} for title,_ in files]}}
            if reply_toggle is not None and reply_toggle.get_active():params['reply_to']={'message_id':original[1]}
            def accepted(result,error):
                if self.closing:files.clear();return
                if error:
                    error_label.set_text(safe_text(error.get('message','Attachments were not accepted.'))+' Close and check history before trying again.')
                    files.clear();return
                # The message text/reply has been posted even if a later upload fails.
                if rid==self.room_id and (self.text(),self.reply,self.mention_ids)==original:
                    self.editor.get_buffer().set_text('');self.cancel_reply();self.mention_ids=[]
                self.schedule_state()
                if dismissed[0]:
                    files.clear();self.transfer_state('Message posted; file uploads cancelled. Check history before retrying.');return
                win.close();self.attachment_dialog=None
                embeds=result.get('embeds',[]) if isinstance(result,dict) else []
                try:
                    if not isinstance(embeds,list) or len(embeds)!=len(files):raise ValueError('The server did not accept every attachment.')
                    for embed in embeds:
                        if not isinstance(embed,dict) or embed.get('kind')!='upload':raise ValueError('Invalid upload response.')
                        transfer_url(embed.get('write_url'),self.session.url)
                except ValueError as error:
                    files.clear();self.transfer_state('Message posted. '+str(error));return
                total=len(files)
                def upload(index=0):
                    if self.closing:files.clear();return
                    if index==total:
                        files.clear();self.transfer_state(f'{total} attachment'+('s' if total!=1 else '')+' uploaded.');return
                    if generation!=self.session.generation or not self.session.ready:
                        files.clear();self.transfer_state(f'{index} of {total} uploaded. Connection changed; remaining files were not sent.');return
                    def finished(_,error):
                        if error:
                            files.clear()
                            if not self.closing:self.transfer_state(f'{index} of {total} uploaded. Message already posted. '+error)
                            return
                        files[index]=(files[index][0],b'')
                        upload(index+1)
                    try:
                        self.transfers.request('PUT',embeds[index]['write_url'],self.session.url,finished,files[index][1])
                        self.transfer_state(f'Uploading file {index+1} of {total}…')
                    except ValueError as error:
                        files.clear();self.transfer_state(f'{index} of {total} uploaded. Message already posted. '+str(error))
                upload()
            self.session.request('message',params,accepted)
        footer=box();footer.set_halign(Gtk.Align.END)
        footer.append(button('Cancel',win.close,'flat'))
        control=button('Send attachments',submit,'suggested-action');footer.append(control);root.append(footer)
        win.present();return win

    def download_attachment(self,embed):
        from .app import Gtk
        if self.transfers.active or self.picking_file:return
        try:url=transfer_url(embed.get('url'),self.session.url)
        except ValueError as error:self.transfer_state(str(error));return
        generation=self.session.generation
        self.picking_file=True
        chooser=Gtk.FileDialog(title='Save attachment · maximum 8 MiB',initial_name=file_title(embed.get('title') or 'attachment'))
        def chosen(dialog,result):
            self.picking_file=False
            try:file=dialog.save_finish(result)
            except GLib.Error:return
            if self.closing or generation!=self.session.generation:return
            if not file.get_path():self.transfer_state('Choose a local destination.');return
            def downloaded(data,error):
                if self.closing:return
                if error:self.transfer_state(error);return
                if generation!=self.session.generation:self.transfer_state('Connection changed; file was not saved.');return
                def saved(_,error):
                    if not self.closing:self.transfer_state(error or 'Attachment saved.')
                try:self.transfers.write_file(file,data,saved);self.transfer_state('Saving attachment…')
                except ValueError as error:self.transfer_state(str(error))
            try:self.transfers.request('GET',url,self.session.url,downloaded);self.transfer_state('Downloading attachment…')
            except ValueError as error:self.transfer_state(str(error))
        chooser.save(self,None,chosen)
