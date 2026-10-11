pub(crate) mod daily;
mod discover;
mod edit;
mod email;
mod focus;
mod images;
mod links;
mod local;
mod manage;
mod mentions;
mod presence;
pub(crate) mod switcher;
use apron_gpui::{
    attention::Attention,
    auth::Secret,
    media::Attachment,
    model::{self, Message, Store},
    transport::{Bridge, Command, Event},
};
pub(crate) use edit::SaveEdit;
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Selectable, Sizable, WindowExt,
    button::ButtonVariants,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{ContextMenuExt, DropdownMenu, PopupMenu},
    message_scroller::{MessageScroller, MessageScrollerState},
    text::TextView,
};
use gpui_kit::{prelude::*, *};
use serde_json::Value;
use std::{collections::BTreeMap, rc::Rc, sync::Arc, time::Duration};

actions!(
    apron,
    [
        SendMessage,
        LoadOlder,
        Latest,
        CancelReply,
        ToggleRooms,
        ReplyMessage,
        CopyMessage,
        OpenLinks,
        FindRoom,
        MarkRead,
        OpenSession,
        OpenEmail,
        ShowHelp,
        OpenPrivacy,
        EditMessage,
        DeleteMessage,
        MentionNext,
        MentionPrevious,
        MentionAccept,
        PasteImage,
        NextMention,
        FindMessages,
        NextMatch,
        PreviousMatch,
        ToggleTheme,
        ShowGallery,
        ToggleNotifications,
        ToggleMute,
        PresenceOnline,
        PresenceNone,
        CreateRoom,
        CreateThread,
        RoomSettings,
        AddMember,
        RemoveMember,
        LeaveRoom,
        ShowMembers,
        ReactLike,
        ReactHeart,
        ReactCelebrate,
        ReactLaugh
    ]
);
type FocusGeometry = Rc<std::cell::RefCell<Option<(u64, Bounds<Pixels>)>>>;
pub struct Conversation {
    pub(crate) store: Store,
    pub(crate) rows: Rc<Vec<Arc<Message>>>,
    pub(crate) scroller: Entity<MessageScrollerState>,
    pub(crate) composer: Entity<TextareaState>,
    bridge: Bridge,
    local_worker: apron_gpui::local_state::Worker,
    profile: Option<apron_gpui::local_state::Profile>,
    local_enabled: bool,
    local_loading: bool,
    local_epoch: u64,
    local_status: String,
    local_error: String,
    local_save: Option<Task<()>>,
    draft_revision: u64,
    privacy_form: Option<WeakEntity<local::PrivacyForm>>,
    anchor_unread: bool,
    local_touched: std::collections::BTreeSet<String>,
    mutation: Option<WeakEntity<edit::MutationForm>>,
    mutation_serial: u64,
    pub(crate) manage_form: Option<WeakEntity<manage::ManageForm>>,
    pub(crate) switcher_form: Option<WeakEntity<switcher::Switcher>>,
    mutation_pending: Option<u64>,
    manage_pending: Option<u64>,
    own_status: String,
    notifications: bool,
    last_notification: Option<std::time::Instant>,
    last_typing: Option<std::time::Instant>,
    typing: BTreeMap<(String, String), std::time::Instant>,
    typing_timer: Option<Task<()>>,
    mutes: BTreeMap<String, Option<std::time::Instant>>,
    server: String,
    pub(crate) remember_session: bool,
    pub(crate) authenticated: bool,
    pub(crate) attention: Attention,
    pub(crate) filter: Entity<InputState>,
    pub(crate) search: Entity<InputState>,
    pub(crate) searching: bool,
    search_index: usize,
    search_matches: Vec<u64>,
    search_query: String,
    unread_only: bool,
    pub(crate) reactions: apron_gpui::reactions::Reactions,
    reacting: Option<u64>,
    pub(crate) session_form: Option<WeakEntity<crate::session::SessionForm>>,
    rooms: BTreeMap<String, Value>,
    users: BTreeMap<String, Value>,
    draft_mentions: BTreeMap<String, std::collections::BTreeSet<String>>,
    mention_index: usize,
    pub(crate) room: String,
    pub(crate) ready: bool,
    pub(crate) loading: bool,
    pending: bool,
    pub(crate) generation: u64,
    before: Option<u64>,
    more: bool,
    pub(crate) reply: Option<u64>,
    you: String,
    drafts: BTreeMap<String, (String, Option<u64>)>,
    files: Vec<Attachment>,
    staged_images: BTreeMap<String, Arc<Image>>,
    image_cache: BTreeMap<String, Arc<Image>>,
    image_errors: BTreeMap<String, String>,
    clipboard_task: Option<Task<()>>,
    clipboard_serial: u64,
    caps: Vec<String>,
    status: String,
    pub(crate) error: String,
    transfer: String,
    transfer_busy: bool,
    sidebar: Option<bool>,
    collapsed: std::collections::BTreeSet<String>,
    open_message: Option<u64>,
    conversation_menu_open: bool,
    pub(crate) rendered_rows: std::cell::Cell<usize>,
    pub(crate) max_render_us: u128,
    pub(crate) transcript_bounds: Rc<std::cell::RefCell<Bounds<Pixels>>>,
    pub(crate) focused_row: FocusGeometry,
    pub(crate) geometry: Rc<std::cell::RefCell<BTreeMap<u64, Bounds<Pixels>>>>,
    pub(crate) inspect: bool,
    _events: Task<()>,
    _subscriptions: Vec<Subscription>,
}
impl Conversation {
    pub fn new(url: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let bridge = Bridge::start(url.clone()).expect("validated server address");
        let events = bridge.events.clone();
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Write a message…")
                .auto_grow(1, 5)
        });
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a room…"));
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search loaded messages…"));
        let search_observer = cx.observe_in(&search, window, |v, _, _, cx| {
            if v.search.read(cx).value().trim().to_lowercase() != v.search_query {
                v.search_index = 0;
                v.update_matches(cx);
                v.reveal_match(cx);
                cx.notify();
            }
        });
        let search_watch = cx.subscribe_in(&search, window, |v, _, event, w, cx| {
            if matches!(event, InputEvent::Change) {
                v.search_index = 0;
                v.update_matches(cx);
                v.reveal_match(cx);
                cx.notify();
            }
            if matches!(event, InputEvent::PressEnter { .. }) {
                v.step_match(true, cx);
                v.search.update(cx, |s, cx| s.focus(w, cx));
            }
        });
        let filter_subscription = cx.observe(&filter, |_, _, cx| cx.notify());
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        let subscription = cx.subscribe_in(&composer, window, |this, _, event, window, cx| {
            if matches!(
                event,
                InputEvent::PressEnter {
                    secondary: true,
                    ..
                }
            ) {
                this.send(window, cx);
            }
            if matches!(event, InputEvent::Change) {
                this.activity_changed(cx);
                this.draft_revision += 1;
                this.local_touched.insert(this.room.clone());
                if !this.room.is_empty() {
                    this.drafts.insert(
                        this.room.clone(),
                        (this.composer.read(cx).value().to_string(), this.reply),
                    );
                }
                this.schedule_local(cx);
            }
            cx.notify();
        });
        let quit = cx.on_app_quit(|this, cx| {
            this.local_save = None;
            let rx = if this.local_enabled {
                this.profile.clone().and_then(|p| {
                    this.local_worker
                        .submit(apron_gpui::local_state::Operation::Save(
                            p,
                            this.local_snapshot(cx),
                        ))
                        .ok()
                })
            } else {
                None
            };
            async move {
                if let Some(rx) = rx {
                    let _ = rx.recv().await;
                }
            }
        });
        window.on_window_should_close(cx, |_, cx| {
            cx.quit();
            false
        });
        let observation = cx.observe_in(&scroller, window, |this, _, window, cx| {
            this.read_visible(window, cx);
            cx.notify();
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.read_visible(window, cx);
        });
        let task = cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this
                    .update_in(cx, |this, window, cx| this.event(event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            store: Store::default(),
            rows: Rc::new(Vec::new()),
            scroller,
            composer,
            bridge,
            local_worker: apron_gpui::local_state::Worker::start(
                apron_gpui::local_state::Backend::default_location(),
            ),
            profile: None,
            local_enabled: false,
            local_loading: false,
            local_epoch: 0,
            local_status: String::new(),
            local_error: String::new(),
            local_save: None,
            draft_revision: 0,
            privacy_form: None,
            anchor_unread: false,
            local_touched: Default::default(),
            mutation: None,
            mutation_serial: 0,
            manage_form: None,
            switcher_form: None,
            mutation_pending: None,
            manage_pending: None,
            own_status: String::new(),
            notifications: false,
            last_notification: None,
            last_typing: None,
            typing: Default::default(),
            typing_timer: None,
            mutes: Default::default(),
            server: url,
            remember_session: false,
            authenticated: false,
            attention: Attention::default(),
            search,
            searching: false,
            search_index: 0,
            search_matches: Vec::new(),
            search_query: String::new(),
            unread_only: false,
            reactions: Default::default(),
            reacting: None,
            filter,
            session_form: None,
            rooms: BTreeMap::new(),
            users: Default::default(),
            draft_mentions: Default::default(),
            mention_index: 0,
            room: String::new(),
            ready: false,
            loading: false,
            pending: false,
            generation: 0,
            before: None,
            more: false,
            reply: None,
            you: String::new(),
            drafts: BTreeMap::new(),
            files: Vec::new(),
            staged_images: BTreeMap::new(),
            image_cache: BTreeMap::new(),
            image_errors: BTreeMap::new(),
            clipboard_task: None,
            clipboard_serial: 0,
            caps: Vec::new(),
            status: "Connecting…".into(),
            error: String::new(),
            transfer: String::new(),
            transfer_busy: false,
            sidebar: None,
            collapsed: Default::default(),
            open_message: None,
            conversation_menu_open: false,
            rendered_rows: std::cell::Cell::new(0),
            max_render_us: 0,
            transcript_bounds: Default::default(),
            focused_row: Default::default(),
            geometry: Rc::new(std::cell::RefCell::new(BTreeMap::new())),
            inspect: false,
            _events: task,
            _subscriptions: vec![
                search_observer,
                search_watch,
                subscription,
                observation,
                activation,
                filter_subscription,
                quit,
            ],
        }
    }
    fn command(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        if self.bridge.commands.try_send(command).is_err() {
            self.error = "Too many pending requests. Try again after the current operation.".into();
            cx.notify();
            false
        } else {
            true
        }
    }
    fn read_visible(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ready
            && !self.local_loading
            && !self.loading
            && !window.has_active_dialog(cx)
            && window.is_window_active()
            && self.scroller.read(cx).is_following_tail()
        {
            self.mark_read(cx);
        }
    }
    fn mark_read(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.rows.last().map(|m| m.id)
            && self.attention.mark_read(&self.room, id)
        {
            // Best-effort activity never crowds out a user command or shows a queue error.
            let _ = self.bridge.commands.try_send(Command::Read {
                room: self.room.clone(),
                through: id,
            });
            self.schedule_local(cx);
            cx.notify();
        }
    }
    pub(crate) fn connect_to(
        &mut self,
        server: String,
        token: Option<Secret>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_change_session() || apron_gpui::media::endpoint(&server).is_err() {
            return;
        }
        self.flush_local(cx);
        self.server = server;
        self.change_session(token, w, cx);
    }
    pub(crate) fn change_session(
        &mut self,
        token: Option<Secret>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_change_session() {
            self.error =
                "Finish the current send or cancel the transfer before changing sessions.".into();
            cx.notify();
            return;
        }
        self.stop_typing();
        self.typing.clear();
        self.typing_timer = None;
        self.mutes.clear();
        self.flush_local(cx);
        self.local_save = None;
        self.local_epoch += 1;
        self.profile = None;
        self.local_enabled = false;
        self.local_loading = false;
        self.draft_revision = 0;
        self.local_touched.clear();
        let bridge =
            match Bridge::start_with_storage(self.server.clone(), token, self.remember_session) {
                Ok(b) => b,
                Err(_) => {
                    self.error = "Could not start the session.".into();
                    cx.notify();
                    return;
                }
            };
        let events = bridge.events.clone();
        self.bridge = bridge;
        self._events = cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this
                    .update_in(cx, |v, w, cx| v.event(event, w, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.generation += 1;
        self.ready = false;
        self.loading = false;
        self.error.clear();
        self.transfer.clear();
        self.status = "Connecting…".into();
        self.room.clear();
        self.rooms.clear();
        self.store = Store::default();
        self.rows = Rc::new(Vec::new());
        self.attention = Attention::default();
        self.drafts.clear();
        self.draft_mentions.clear();
        self.users.clear();
        self.files.clear();
        self.staged_images.clear();
        self.image_cache.clear();
        self.image_errors.clear();
        self.clipboard_task = None;
        self.reply = None;
        self.reactions = Default::default();
        self.search_matches.clear();
        self.searching = false;
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        self.you.clear();
        self.composer
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.scroller.update(cx, |s, cx| s.reset(0, cx));
        cx.notify();
    }
    pub(crate) fn can_change_session(&self) -> bool {
        !self.pending
            && !self.transfer_busy
            && self.mutation_pending.is_none()
            && self.manage_pending.is_none()
            && self.reacting.is_none()
    }
    pub(crate) fn session_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let owner = cx.weak_entity();
        let server = self.server.clone();
        let form = cx.new(|cx| crate::session::SessionForm::new(owner, server, window, cx));
        self.session_form = Some(form.downgrade());
        window.open_dialog(cx, move |dialog, _, cx| {
            let clear = form.clone();
            dialog
                .title("Sign in to Apron")
                .child(form.clone())
                .footer(form.update(cx, |f, cx| f.actions(cx)))
                .on_close(move |_, window, cx| clear.update(cx, |f, cx| f.clear(window, cx)))
        });
    }
    fn help_dialog(window: &mut Window, cx: &mut App) {
        window.open_dialog(cx,|dialog,_,_|dialog.title("Keyboard & message actions").child(div().flex().flex_col().gap_3()
            .child("Ctrl+Enter — send · Enter — new line")
            .child("Ctrl+K — find a room · Ctrl+B — toggle navigation")
            .child("Ctrl+F — search loaded messages · F3 / Shift+F3 — next / previous match")
            .child("Ctrl+PageUp — earlier history · Ctrl+PageDown — latest")
            .child("Right-click a message, or Tab to its actions button and press Enter")
            .child("With a message focused: R — reply · Ctrl+Shift+C — copy message")
            .child("Ctrl+Shift+U — mark this room read")
            .child("Ctrl+C — copy selected text · Escape — close menu or cancel reply")
            .child("Unread counts cover received and fetched messages. Mention badges use server-provided mention IDs, not matching text.")));
    }
    fn listing(&mut self, list: &Value) {
        model::update_rooms(&mut self.rooms, list);
        if let Some(users) = list["users"].as_array() {
            for user in users.iter().take(1024) {
                if let Some(id) = user["user_id"].as_str().filter(|id| id.len() <= 128) {
                    self.users.insert(id.into(),serde_json::json!({"user_id":id,"name":model::clean(user["name"].as_str().unwrap_or(id),80).replace(['\n','\t']," ")}));
                }
            }
        }
        while self.users.len() > 1024 {
            self.users.pop_first();
        }
    }
    pub(crate) fn select(&mut self, room: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.room.is_empty() {
            self.drafts.insert(
                self.room.clone(),
                (self.composer.read(cx).value().to_string(), self.reply),
            );
        }
        while self.drafts.len() > 256 {
            self.drafts.pop_first();
        }
        self.schedule_local(cx);
        self.stop_typing();
        self.anchor_unread = true;
        self.mention_index = 0;
        self.reacting = None;
        self.search_index = 0;
        self.room = room;
        self.generation += 1;
        self.loading = false;
        self.pending = false;
        self.files.clear();
        self.staged_images.clear();
        self.image_cache.clear();
        self.image_errors.clear();
        self.clipboard_task = None;
        self.transfer.clear();
        self.before = None;
        self.more = false;
        let (text, reply) = self.drafts.get(&self.room).cloned().unwrap_or_default();
        self.reply = reply;
        self.composer
            .update(cx, |s, cx| s.set_value(text, window, cx));
        self.rows = Rc::new(self.store.rows(&self.room));
        self.update_matches(cx);
        self.scroller
            .update(cx, |s, cx| s.reset(self.rows.len(), cx));
        self.history(None, cx);
        if window.viewport_size().width < px(crate::design::COMPACT) {
            self.sidebar = None;
        }
        cx.notify();
    }
    fn history(&mut self, before: Option<u64>, cx: &mut Context<Self>) {
        if !self.ready || self.loading || self.room.is_empty() {
            return;
        }
        if self.command(
            Command::History {
                generation: self.generation,
                room: self.room.clone(),
                before,
            },
            cx,
        ) {
            self.loading = true;
            self.error.clear();
            cx.notify();
        }
    }
    pub(crate) fn older(&mut self, cx: &mut Context<Self>) {
        if self.more {
            self.history(self.before, cx);
        }
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let next = self.store.rows(&self.room);
        let changes = model::splices(&self.rows, &next);
        let old: BTreeMap<_, _> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, (m.log, i > 0 && model::grouped(&self.rows[i - 1], m))))
            .collect();
        let changed: Vec<_> = next
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                old.get(&m.id).is_some_and(|(log, group)| {
                    *log != m.log || *group != (*i > 0 && model::grouped(&next[*i - 1], m))
                })
            })
            .map(|(i, _)| i)
            .collect();
        self.rows = Rc::new(next);
        self.update_matches(cx);
        self.scroller.update(cx, |s, cx| {
            for (range, count) in changes {
                s.splice(range, count, cx);
            }
            for i in changed {
                s.remeasure_items(i..i + 1, cx);
            }
        });
    }
    pub(crate) fn event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Identity(you) => {
                self.own_status = model::clean(model::text(&you["status"]), 40);
            }
            Event::Managed { operation, result } => {
                if self.manage_pending == Some(operation) {
                    self.manage_pending = None;
                }
                if let Ok(value) = &result
                    && let Some(room) = value["room_id"].as_str()
                    && self.rooms.contains_key(room)
                    && room != self.room
                {
                    self.select(room.to_owned(), window, cx);
                }

                if let Some(form) = self.manage_form.as_ref().and_then(|f| f.upgrade()) {
                    form.update(cx, |f, cx| f.finish(operation, result.clone(), window, cx));
                }
                if let Ok(value) = &result
                    && value["you"].is_object()
                {
                    self.own_status = model::clean(model::text(&value["you"]["status"]), 40);
                }
                if let Err(e) = result {
                    self.error = e;
                }
            }

            Event::RecoveryGap { epoch, message } => {
                if epoch == self.local_epoch {
                    self.error = message;
                }
            }
            Event::Recovered { epoch, room, page } => {
                if epoch != self.local_epoch {
                    return;
                }
                let rows = page["messages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| Message::parse(v.clone()).map(Arc::new))
                    .filter(|m| m.room == room)
                    .collect::<Vec<_>>();
                self.attention.history(&room, &rows, &self.you);
                self.schedule_local(cx);
            }

            Event::Reacted {
                generation,
                message,
                result,
            } => {
                if generation == self.generation && self.reacting == Some(message) {
                    self.reacting = None;
                    if let Err(e) = result {
                        self.error = e;
                    }
                }
            }
            Event::Mutated { operation, result } => {
                if self.mutation_pending == Some(operation) {
                    self.mutation_pending = None;
                }
                if let Some(form) = self.mutation.as_ref().and_then(|f| f.upgrade()) {
                    form.update(cx, |form, cx| {
                        form.finished(operation, result.clone(), window, cx)
                    });
                }
                if let Err(error) = result {
                    self.error = error;
                }
            }

            Event::State(s) => {
                if let Some(op) = self.manage_pending.take()
                    && let Some(form) = self.manage_form.as_ref().and_then(|f| f.upgrade())
                {
                    form.update(cx,|f,cx|f.finish(op,Err("Connection changed; completion is uncertain. Inspect the room before retrying.".into()),window,cx));
                }

                if let Some(operation) = self.mutation_pending.take()
                    && let Some(form) = self.mutation.as_ref().and_then(|f| f.upgrade())
                {
                    form.update(cx,|f,cx|f.finished(operation,Err("Connection changed; completion may be uncertain. Inspect the message before retrying.".into()),window,cx));
                }
                self.reacting = None;
                self.typing.clear();
                self.typing_timer = None;
                self.last_typing = None;
                if s == "Connecting…" {
                    self.mutes.clear();
                    self.own_status.clear();
                    for user in self.users.values_mut() {
                        if let Some(o) = user.as_object_mut() {
                            o.remove("status");
                        }
                    }
                }
                self.status = s;
                self.ready = false;
                self.loading = false;
                self.pending = false;
                self.generation += 1;
                self.transfer_busy = false;
            }
            Event::Ready {
                you,
                authenticated,
                rooms,
                caps,
            } => {
                self.authenticated = authenticated;
                if self.you != you {
                    self.attention = Attention::default();
                }
                self.you = you;
                self.load_profile(window, cx);
                self.rooms.clear();
                self.listing(&rooms);
                self.caps = caps;
                self.ready = true;
                self.status = "Connected".into();
                self.reactions = Default::default();
                self.store = Store::default();
                self.rows = Rc::new(Vec::new());
                let room = if self.rooms.contains_key(&self.room) {
                    self.room.clone()
                } else if self.rooms.contains_key("general") {
                    "general".into()
                } else {
                    self.rooms.keys().next().cloned().unwrap_or_default()
                };
                self.select(room, window, cx);
                if !self.local_loading {
                    self.recover_attention(cx);
                }
            }
            Event::Notice(frame) => match frame["method"].as_str() {
                Some("message") => {
                    let raw = &frame["params"];
                    self.typing.remove(&(
                        model::text(&raw["room_id"]).to_owned(),
                        model::text(&raw["from"]["user_id"]).to_owned(),
                    ));
                    let id = model::number(&raw["message_id"]);
                    let old = id.and_then(|id| self.store.get(id)).cloned();
                    if self.store.apply(raw.clone()) {
                        if let Some(message) = id.and_then(|id| self.store.get(id)).cloned() {
                            if let Some(old) = &old
                                && old.room != message.room
                            {
                                self.attention.remove(&old.room, old.id);
                            }
                            let mentioned = raw["body"]["mentions"]
                                .as_array()
                                .is_some_and(|v| v.iter().any(|id| id == &self.you));
                            let previously = old.as_ref().is_some_and(|m| {
                                m.raw["body"]["mentions"]
                                    .as_array()
                                    .is_some_and(|v| v.iter().any(|id| id == &self.you))
                            });
                            if message.deleted() {
                                self.attention.remove(&message.room, message.id);
                            } else {
                                if old.is_none() && raw["from"]["user_id"] != self.you {
                                    self.notify_message(&message.room, window, cx);
                                }
                                self.attention.receive(
                                    &message.room,
                                    message.id,
                                    raw["from"]["user_id"] == self.you,
                                    mentioned,
                                    old.is_some() && mentioned && !previously,
                                );
                            }
                        }
                        if let Some(message) = id.and_then(|id| self.store.get(id)).cloned() {
                            self.attention.history(
                                &message.room,
                                std::slice::from_ref(&message),
                                &self.you,
                            );
                        }
                        self.schedule_local(cx);
                        self.refresh(cx);
                        self.read_visible(window, cx);
                    }
                }
                Some("status") => self.status_notice(&frame["params"]),
                Some("user") => {
                    if frame["params"]["you"].is_object() {
                        self.own_status =
                            model::clean(model::text(&frame["params"]["you"]["status"]), 40);
                    }
                    let user = &frame["params"]["new"];
                    let id = model::text(&user["user_id"]);
                    if !id.is_empty()
                        && id.len() <= 128
                        && (self.users.len() < 1024 || self.users.contains_key(id))
                    {
                        self.users.insert(id.into(), user.clone());
                    }
                }
                Some("activity") if frame["params"]["from"]["user_id"] != self.you => {
                    self.receive_activity(&frame["params"], cx)
                }
                Some("reactions") => {
                    self.reactions.apply(&frame["params"]);
                    if let Some(id) = model::number(&frame["params"]["message_id"])
                        && let Some(ix) = self.rows.iter().position(|m| m.id == id)
                    {
                        self.scroller
                            .update(cx, |s, cx| s.remeasure_items(ix..ix + 1, cx));
                    }
                }
                Some("room_update") => self.listing(&frame["params"]),
                Some("activity") if frame["params"]["from"]["user_id"] == self.you => {
                    if let (Some(room), Some(id)) = (
                        frame["params"]["room_id"].as_str(),
                        model::number(&frame["params"]["read_message_id"]),
                    ) {
                        self.attention.mark_read(room, id);
                        self.schedule_local(cx);
                    }
                }
                _ => {}
            },
            Event::History {
                generation,
                room,
                page,
            } => {
                if generation != self.generation || room != self.room {
                    return;
                }
                self.loading = false;
                self.before = model::number(&page["first_log_id"]).and_then(|n| n.checked_sub(1));
                self.more = page["more"] == true && self.before.is_some();
                if let Some(ms) = page["messages"].as_array() {
                    for m in ms {
                        self.store.apply(m.clone());
                    }
                }
                let mut reaction_ids = std::collections::BTreeSet::new();
                for r in page["reactions"].as_array().into_iter().flatten() {
                    self.reactions.apply(r);
                    if let Some(id) = model::number(&r["message_id"]) {
                        reaction_ids.insert(id);
                    }
                }
                self.refresh(cx);
                let affected: Vec<_> = self
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| reaction_ids.contains(&m.id))
                    .map(|(i, _)| i)
                    .collect();
                if !affected.is_empty() {
                    self.scroller.update(cx, |s, cx| {
                        for i in affected {
                            s.remeasure_items(i..i + 1, cx);
                        }
                    });
                }
                let anchor = self.anchor_unread;
                self.anchor_unread = false;
                self.reconcile_attention(window, cx, anchor);
            }
            Event::Sent {
                generation,
                text,
                reply,
            } => {
                if generation == self.generation {
                    self.pending = false;
                    self.files.clear();
                    self.staged_images.clear();
                    if self.composer.read(cx).value().as_ref() == text && self.reply == reply {
                        self.composer
                            .update(cx, |s, cx| s.set_value("", window, cx));
                        self.reply = None;
                        self.draft_mentions.remove(&self.room);
                    }
                    self.transfer.clear();
                    self.schedule_local(cx);
                }
            }
            Event::Image {
                generation,
                key,
                result,
            } => {
                if generation == self.generation && self.image_key_current(&key) {
                    match result {
                        Ok(raster) => {
                            if self.image_cache.len() >= 8 {
                                self.image_cache.clear();
                            }
                            self.image_errors.remove(&key);
                            self.image_cache.insert(
                                key,
                                Arc::new(Image::from_bytes(ImageFormat::Png, raster.thumbnail)),
                            );
                        }
                        Err(error) => {
                            if self.image_errors.len() >= 16 {
                                self.image_errors.clear();
                            }
                            self.image_errors.insert(key, model::clean(&error, 240));
                        }
                    }
                }
            }
            Event::Files { generation, files } => {
                if generation == self.generation {
                    self.staged_images.clear();
                    self.files = files;
                    self.transfer = "Files ready. Review them below before sending.".into();
                }
            }
            Event::Failure {
                generation,
                message,
            } => {
                if generation.is_none_or(|g| g == self.generation) {
                    self.loading = false;
                    self.pending = false;
                    self.error = model::clean(&message, 500);
                }
            }
            Event::Transfer { message, busy } => {
                self.transfer = message;
                self.transfer_busy = busy;
            }
        };
        cx.notify();
    }
    pub(crate) fn send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        if !self.ready
            || self.pending
            || (!self.joined())
            || (text.trim().is_empty() && self.files.is_empty())
        {
            return;
        }
        if text.len() > 32000 {
            self.error = "Message exceeds 32,000 bytes.".into();
            cx.notify();
            return;
        }
        if self.command(
            Command::Send {
                generation: self.generation,
                room: self.room.clone(),
                mentions: self
                    .draft_mentions
                    .get(&self.room)
                    .map(|ids| apron_gpui::mentions::recipients(&text, ids))
                    .unwrap_or_default(),
                text,
                reply: self.reply,
                files: self.files.clone(),
            },
            cx,
        ) {
            self.stop_typing();
            self.pending = true;
            self.error.clear();
            cx.notify();
        }
    }
    pub(crate) fn begin_reply(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || !self.joined() {
            return;
        }
        self.reply = Some(id);
        self.composer.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }
    fn joined(&self) -> bool {
        self.rooms
            .get(&self.room)
            .is_some_and(|r| r["_joined"] == true)
    }
    fn attach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.generation;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Choose up to four files · 8 MiB total".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if generation != this.generation {
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) => {
                        this.command(Command::ReadFiles { generation, paths }, cx);
                    }
                    Ok(Ok(None)) => {}
                    _ => {
                        this.error =
                            "File chooser unavailable. A desktop portal is required.".into();
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn download(
        &mut self,
        url: String,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let generation = self.generation;
        let prompt = cx.prompt_for_new_path(std::path::Path::new("."), Some(&title));
        cx.spawn_in(window, async move |this, cx| {
            let result = prompt.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if generation != this.generation {
                    return;
                }
                match result {
                    Ok(Ok(Some(path))) => {
                        this.command(
                            Command::Download {
                                generation,
                                url,
                                path,
                            },
                            cx,
                        );
                    }
                    Ok(Ok(None)) => {}
                    _ => {
                        this.error =
                            "Save dialog unavailable. A desktop portal is required.".into();
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn row(
        m: Arc<Message>,
        grouped: bool,
        owner: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let id = m.id;
        let focus_owner = owner.clone();
        let focus = window.use_keyed_state(("row-focus", id), cx, |w, cx| {
            focus::RowFocus::new(id, focus_owner, w, cx)
        });
        let focus_bounds = focus.clone();
        let focus_record = owner
            .upgrade()
            .and_then(|v| v.read(cx).inspect.then(|| v.read(cx).focused_row.clone()));
        let raw = &m.raw;
        let copy = model::clean(model::text(&raw["body"]["text"]), 32000);
        let name = model::clean(
            raw["from"]["name"]
                .as_str()
                .or(raw["from"]["user_id"].as_str())
                .unwrap_or("Unknown"),
            80,
        );
        let timestamp = i64::try_from(id)
            .ok()
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%b %-d · %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let initial = name
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        let avatar_color = crate::design::avatar(model::text(&raw["from"]["user_id"]), cx);
        let reaction_owner = owner.clone();
        let matched = owner.upgrade().is_some_and(|v| {
            v.read(cx).searching
                && v.read(cx).match_ids(cx).get(v.read(cx).search_index) == Some(&id)
        });
        let edit_owner = owner.clone();
        let delete_owner = owner.clone();
        let reply_owner = owner.clone();
        let menu_owner = owner.clone();
        let context_owner = owner.clone();
        let open_owner = owner.clone();
        let menu_open = owner
            .upgrade()
            .is_some_and(|v| v.read(cx).open_message == Some(id));
        let group: SharedString = format!("message-{id}").into();
        let mentioned = owner.upgrade().is_some_and(|v| {
            raw["body"]["mentions"]
                .as_array()
                .is_some_and(|ms| ms.iter().any(|id| id == &v.read(cx).you))
        });
        let mut row = div()
            .id(("message", id))
            .group(group.clone())
            .track_focus(&focus.read(cx).handle)
            .relative()
            .child(
                canvas(
                    move |bounds, w, cx| {
                        if focus_bounds.read(cx).handle.contains_focused(w, cx)
                            && let Some(record) = &focus_record
                        {
                            *record.borrow_mut() = Some((id, bounds));
                        }
                        focus_bounds.update(cx, |state, _| state.bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .key_context("Message")
            .on_action(move |_: &ReplyMessage, window, cx| {
                let _ = reply_owner.update(cx, |v, cx| v.begin_reply(id, window, cx));
            })
            .on_action({
                let owner = owner.clone();
                move |_: &OpenLinks, w, cx| {
                    let _ = owner.update(cx, |v, cx| v.links_dialog(id, w, cx));
                }
            })
            .on_action(move |_: &EditMessage, w, cx| {
                let _ = edit_owner.update(cx, |v, cx| v.open_mutation(id, false, w, cx));
            })
            .on_action(move |_: &DeleteMessage, w, cx| {
                let _ = delete_owner.update(cx, |v, cx| v.open_mutation(id, true, w, cx));
            })
            .on_action(move |_: &CopyMessage, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
            })
            .on_action(move |_: &ReactLike, _, cx| {
                let _ = reaction_owner.update(cx, |v, cx| v.react(id, "👍", cx));
            })
            .on_action({
                let o = owner.clone();
                move |_: &ReactHeart, _, cx| {
                    let _ = o.update(cx, |v, cx| v.react(id, "❤️", cx));
                }
            })
            .on_action({
                let o = owner.clone();
                move |_: &ReactCelebrate, _, cx| {
                    let _ = o.update(cx, |v, cx| v.react(id, "🎉", cx));
                }
            })
            .on_action({
                let o = owner.clone();
                move |_: &ReactLaugh, _, cx| {
                    let _ = o.update(cx, |v, cx| v.react(id, "😂", cx));
                }
            })
            .rounded_lg()
            .px_3()
            .border_l_2()
            .border_color(if mentioned || matched {
                cx.theme().primary
            } else {
                cx.theme().transparent
            })
            .when(mentioned || matched, |d| {
                d.bg(cx.theme().primary.opacity(0.08))
            })
            .hover(|d| d.bg(cx.theme().muted.opacity(0.65)))
            .flex()
            .flex_col()
            .gap_1()
            .min_w_0()
            .py_1()
            .pl(px(60.))
            .when(!grouped, |d| {
                d.mt_3().pt_2().child(
                    div()
                        .absolute()
                        .left_3()
                        .top_2()
                        .size_8()
                        .rounded_full()
                        .bg(avatar_color.opacity(0.12))
                        .text_color(avatar_color)
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(initial),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .when(!grouped, |d| {
                        d.child(
                            div()
                                .max_w_48()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(avatar_color)
                                .child(name),
                        )
                    })
                    .when(!grouped, |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(timestamp.clone()),
                        )
                    })
                    .when(mentioned, |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().primary)
                                .child("@ You"),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        crate::design::button(("message-actions", id))
                            .ghost()
                            .absolute()
                            .right_1()
                            .top_1()
                            .xsmall()
                            .icon(IconName::Ellipsis)
                            .accessibility_label("Message actions")
                            .when(!menu_open, |b| {
                                b.tooltip("Message actions · Tab then Enter")
                            })
                            .opacity(if menu_open { 1. } else { 0. })
                            .group_hover(group, |s| s.opacity(1.))
                            .focus_visible(|s| s.opacity(1.))
                            .dropdown_menu(move |menu, w, cx| {
                                Self::message_menu(menu, id, &menu_owner, w, cx)
                            })
                            .on_open_change(move |open, _, cx| {
                                let _ = open_owner.update(cx, |v, cx| {
                                    v.open_message = (*open).then_some(id);
                                    cx.notify();
                                });
                            }),
                    ),
            );
        if let Some(reply) = model::number(&raw["reply_to"]["message_id"]) {
            let summary = owner
                .upgrade()
                .map(|v| v.read(cx).reply_summary(reply))
                .unwrap_or_else(|| "Earlier message".into());
            row = row.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("↳ {summary}")),
            );
        }
        let geometry = owner.upgrade().and_then(|e| {
            let v = e.read(cx);
            v.inspect.then(|| v.geometry.clone())
        });
        row = row.child(
            div()
                .relative()
                .when_some(geometry, |d, g| {
                    d.child(
                        canvas(
                            move |bounds, _, _| {
                                g.borrow_mut().insert(id, bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                })
                .child(
                    TextView::markdown(("body", id), SharedString::from(m.display.to_string()))
                        .selectable(true)
                        .on_link_click(|_, _, _, _| {}),
                ),
        );
        if let Some(embeds) = raw["body"]["embeds"].as_array() {
            for (ix, embed) in embeds.iter().take(4).enumerate() {
                let title = model::clean(embed["title"].as_str().unwrap_or("Attachment"), 180);
                let target = embed["url"].as_str().map(str::to_owned);
                let owner = owner.clone();
                if embed["kind"] == "upload" {
                    if let Some(url) = target.clone() {
                        row = row.child(Self::embedded_image(id, ix, url, &owner, cx));
                    }
                    row = row.child(
                        crate::design::button(SharedString::from(format!("save-{id}-{ix}")))
                            .self_start()
                            .outline()
                            .small()
                            .label(format!("Save {title}…"))
                            .disabled(target.is_none())
                            .on_click(move |_, window, cx| {
                                if let Some(url) = target.clone() {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.download(url, title.clone(), window, cx)
                                    });
                                }
                            }),
                    );
                }
            }
        }
        if let Some(v) = owner.upgrade()
            && !v.read(cx).reactions.totals(id).is_empty()
        {
            row = row.child(v.read(cx).reaction_chips(id, owner.clone(), cx));
        }
        row.context_menu(move |menu, w, cx| Self::message_menu(menu, id, &context_owner, w, cx))
            .into_any_element()
    }
    fn message_menu(
        menu: PopupMenu,
        id: u64,
        owner: &WeakEntity<Self>,
        w: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let can_reply = owner
            .upgrade()
            .is_some_and(|v| v.read(cx).ready && v.read(cx).joined());
        let can_edit = owner.upgrade().is_some_and(|v| v.read(cx).can_mutate(id));
        let can_react = owner.upgrade().is_some_and(|v| v.read(cx).can_react(id));
        menu.menu_with_disabled("Reply", Box::new(ReplyMessage), !can_reply)
            .menu("Copy message", Box::new(CopyMessage))
            .menu("Open external links…", Box::new(OpenLinks))
            .when(can_react, |m| {
                m.submenu("React…", w, cx, |m, _, _| {
                    m.menu("👍  Like", Box::new(ReactLike))
                        .menu("❤️  Love", Box::new(ReactHeart))
                        .menu("🎉  Celebrate", Box::new(ReactCelebrate))
                        .menu("😂  Laugh", Box::new(ReactLaugh))
                })
            })
            .when(can_edit, |m| {
                m.separator()
                    .menu("Edit message…", Box::new(EditMessage))
                    .menu("Delete message…", Box::new(DeleteMessage))
            })
    }
    fn navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut pane = div()
            .w(px(crate::design::SIDEBAR))
            .min_w_48()
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .h_20()
                    .px_5()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size_9()
                            .rounded_lg()
                            .bg(cx.theme().primary)
                            .text_color(cx.theme().primary_foreground)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .child("a"),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Apron"),
                    ),
            )
            .child(
                div()
                    .px_4()
                    .pb_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("WORKSPACE"),
            );
        let mut rooms = div()
            .id("rooms")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2();
        let query = self.filter.read(cx).value().to_lowercase();
        pane = pane.child(
            div().px_3().pb_3().child(
                Input::new(&self.filter)
                    .prefix(IconName::Search)
                    .aria_label("Find a room")
                    .w_full(),
            ),
        );
        pane = pane.child(
            div()
                .px_3()
                .pb_3()
                .flex()
                .gap_2()
                .child(
                    crate::design::button("all-rooms")
                        .ghost()
                        .small()
                        .label("All rooms")
                        .selected(!self.unread_only)
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.unread_only = false;
                            cx.notify();
                        })),
                )
                .child(
                    crate::design::button("unread-rooms")
                        .ghost()
                        .small()
                        .label("Unread")
                        .selected(self.unread_only)
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.unread_only = true;
                            cx.notify();
                        })),
                ),
        );
        let mut ordered: Vec<_> = self.rooms.iter().collect();
        ordered.sort_by_key(|(id, room)| {
            (
                room["parent_room_id"].as_str().unwrap_or(id).to_owned(),
                room["parent_room_id"].is_string(),
                room["title"].as_str().unwrap_or(id).to_lowercase(),
            )
        });
        let mut matches = 0;
        for (id, room) in ordered {
            if apron_gpui::navigation::score(
                &query,
                &format!("{} {}", id, room["title"].as_str().unwrap_or("")),
            )
            .is_none()
            {
                continue;
            }
            if query.is_empty()
                && room["parent_room_id"]
                    .as_str()
                    .is_some_and(|p| self.collapsed.contains(p))
            {
                continue;
            }
            let children: Vec<_> = self
                .rooms
                .iter()
                .filter(|(_, r)| r["parent_room_id"] == *id)
                .map(|(id, _)| id)
                .collect();
            let (mut unread, mut mentions) = self.attention.counts(id);
            if self.collapsed.contains(id) {
                for child in &children {
                    let (u, m) = self.attention.counts(child);
                    unread += u;
                    mentions += m;
                }
            }
            if self.unread_only && unread == 0 {
                continue;
            }
            matches += 1;
            let target = id.clone();
            let title = model::clean(room["title"].as_str().unwrap_or(id), 80);
            let mut navigation_row = div().flex().items_center().w_full();
            if !children.is_empty() {
                let parent = id.clone();
                navigation_row = navigation_row.child(
                    crate::design::button(SharedString::from(format!("fold-{id}")))
                        .ghost()
                        .w_6()
                        .flex_shrink_0()
                        .label(if self.collapsed.contains(id) {
                            "▸"
                        } else {
                            "▾"
                        })
                        .tooltip(format!("{} threads", children.len()))
                        .accessibility_label(format!("Toggle threads under {title}"))
                        .on_click(cx.listener(move |v, _, _, cx| {
                            if !v.collapsed.remove(&parent) {
                                v.collapsed.insert(parent.clone());
                            }
                            cx.notify();
                        })),
                );
            }
            rooms = rooms.child(
                navigation_row.child(
                    crate::design::button(SharedString::from(format!("room-{id}")))
                        .ghost()
                        .selected(id == &self.room)
                        .flex_1()
                        .min_w_0()
                        .h(px(crate::design::ROW))
                        .mb_1()
                        .when(room["parent_room_id"].is_string(), |b| b.ml_3().w(px(216.)))
                        .when(id == &self.room, |b| {
                            b.bg(cx.theme().accent).text_color(cx.theme().primary)
                        })
                        .when(unread > 0, |b| b.font_weight(FontWeight::SEMIBOLD))
                        .justify_start()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .w_full()
                                .child(div().flex_1().min_w_0().truncate().child(format!(
                                    "{} {title}",
                                    if room["parent_room_id"].is_string() {
                                        "↳"
                                    } else {
                                        "#"
                                    }
                                )))
                                .when(unread > 0, |d| {
                                    d.child(
                                        div()
                                            .px_2()
                                            .rounded_full()
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(if mentions > 0 {
                                                format!("{unread} · @{mentions}")
                                            } else {
                                                unread.to_string()
                                            }),
                                    )
                                }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select(target.clone(), window, cx);
                            this.composer.update(cx, |s, cx| s.focus(window, cx));
                        })),
                ),
            );
        }
        if matches == 0 {
            rooms = rooms.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if !query.is_empty() {
                        "No matching rooms"
                    } else if self.ready {
                        if self.unread_only {
                            "You’re all caught up"
                        } else {
                            "No rooms available"
                        }
                    } else {
                        "Rooms appear after connecting"
                    }),
            );
        }
        pane = pane.child(rooms).child(
            div()
                .m_3()
                .p_3()
                .rounded_lg()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if !self.ready {
                    "Session offline"
                } else if self.authenticated {
                    "Token session"
                } else {
                    "Guest session"
                }),
        );
        pane.into_any_element()
    }
    pub(crate) fn sidebar_visible(&self, window: &Window) -> bool {
        // Automatic compact layout until the user explicitly toggles navigation.
        self.sidebar
            .unwrap_or(window.viewport_size().width >= px(crate::design::COMPACT))
    }
    fn toggle_rooms(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.sidebar = Some(!self.sidebar_visible(window));
        cx.notify();
    }
    fn header(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let title = self
            .rooms
            .get(&self.room)
            .and_then(|r| r["title"].as_str())
            .unwrap_or(&self.room)
            .to_owned();
        let content_width = window.viewport_size().width
            - if self.sidebar_visible(window) {
                window.rem_size() * 16.
            } else {
                px(0.)
            };
        let show_history = content_width >= window.rem_size() * 32.;
        let can_load = self.more && !self.loading && self.ready;
        let notifications = self.notifications;
        let muted = self.muted(&self.room);
        div()
            .h_16()
            .px_6()
            .flex_shrink_0()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .gap_3()
            .child(
                crate::design::button("toggle-rooms")
                    .ghost()
                    .small()
                    .icon(IconName::PanelLeft)
                    .accessibility_label("Toggle rooms")
                    .tooltip("Toggle rooms · Ctrl+B")
                    .selected(self.sidebar_visible(window))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_rooms(window, cx))),
            )
            .child(
                div()
                    .size_8()
                    .rounded_lg()
                    .bg(cx.theme().muted)
                    .text_color(cx.theme().muted_foreground)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_lg()
                    .child(
                        if self
                            .rooms
                            .get(&self.room)
                            .is_some_and(|r| r["parent_room_id"].is_string())
                        {
                            "↳"
                        } else {
                            "#"
                        },
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(
                        if title.is_empty() {
                            "Apron".into()
                        } else {
                            model::clean(&title, 120)
                        },
                    ))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.typing_label().unwrap_or_else(|| self.status.clone())),
                    ),
            )
            .child(
                crate::design::button("find-messages")
                    .ghost()
                    .small()
                    .icon(IconName::Search)
                    .accessibility_label("Search loaded messages")
                    .tooltip("Search loaded messages · Ctrl+F")
                    .on_click(cx.listener(|v, _, w, cx| v.open_search(w, cx))),
            )
            .when(show_history, |d| {
                d.child(
                    crate::design::button("older")
                        .ghost()
                        .small()
                        .label(if self.loading {
                            "Loading…"
                        } else {
                            "Load earlier"
                        })
                        .disabled(!self.more || self.loading || !self.ready)
                        .on_click(cx.listener(|this, _, _, cx| this.older(cx))),
                )
            })
            .when(self.attention.counts(&self.room).0 > 0, |d| {
                let (unread, mentions) = self.attention.counts(&self.room);
                d.child(
                    crate::design::button("unread")
                        .ghost()
                        .small()
                        .label(if mentions > 0 {
                            format!("{unread} unread · @{mentions}")
                        } else {
                            format!("{unread} unread")
                        })
                        .tooltip("Go to first retained unread message")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(id) = this.attention.first_unread(&this.room)
                                && let Some(ix) = this.rows.iter().position(|m| m.id >= id)
                            {
                                this.scroller.update(cx, |s, cx| s.scroll_to_item(ix, cx));
                            }
                        })),
                )
            })
            .when(!self.ready, |d| {
                d.child(
                    crate::design::button("reconnect")
                        .ghost()
                        .small()
                        .label(if self.status == "Sign-in required" {
                            "Sign in…"
                        } else {
                            "Retry"
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.status == "Sign-in required" {
                                this.session_dialog(window, cx);
                            } else {
                                this.command(Command::Reconnect, cx);
                            }
                        })),
                )
            })
            .child(
                crate::design::button("conversation-menu")
                    .ghost()
                    .small()
                    .icon(IconName::Ellipsis)
                    .accessibility_label("Conversation and session actions")
                    .when(!self.conversation_menu_open, |b| {
                        b.tooltip("Conversation and session actions")
                    })
                    .dropdown_menu(move |menu, w, cx| {
                        menu.menu("Mark room read", Box::new(MarkRead))
                            .menu("Jump to latest", Box::new(Latest))
                            .menu_with_disabled("Load earlier", Box::new(LoadOlder), !can_load)
                            .separator()
                            .menu("Sign in with token…", Box::new(OpenSession))
                            .menu("Sign in with email…", Box::new(OpenEmail))
                            .submenu("Room and members", w, cx, |m, _, _| {
                                m.menu("Create room…", Box::new(CreateRoom))
                                    .menu("Create thread…", Box::new(CreateThread))
                                    .menu("Room settings…", Box::new(RoomSettings))
                                    .menu("Members…", Box::new(ShowMembers))
                                    .menu("Add member…", Box::new(AddMember))
                                    .menu("Remove member…", Box::new(RemoveMember))
                                    .menu("Leave room…", Box::new(LeaveRoom))
                            })
                            .menu("Next unread mention", Box::new(NextMention))
                            .submenu("Preferences", w, cx, move |m, _, _| {
                                m.menu(
                                    if notifications {
                                        "Disable desktop notifications"
                                    } else {
                                        "Enable desktop notifications"
                                    },
                                    Box::new(ToggleNotifications),
                                )
                                .menu(
                                    if muted {
                                        "Unmute this room"
                                    } else {
                                        "Mute this room"
                                    },
                                    Box::new(ToggleMute),
                                )
                                .menu("Presence: online", Box::new(PresenceOnline))
                                .menu("Presence: opt out", Box::new(PresenceNone))
                                .menu("Local data…", Box::new(OpenPrivacy))
                                .menu("Switch light / dark", Box::new(ToggleTheme))
                            })
                            .menu("Keyboard shortcuts…", Box::new(ShowHelp))
                            .menu("Component gallery…", Box::new(ShowGallery))
                    })
                    .on_open_change(cx.listener(|this, open, _, cx| {
                        this.conversation_menu_open = *open;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    fn reply_summary(&self, id: u64) -> String {
        self.store
            .get(id)
            .map(|message| {
                let author = model::clean(
                    message.raw["from"]["name"]
                        .as_str()
                        .unwrap_or("Earlier message"),
                    40,
                );
                let excerpt = model::clean(model::text(&message.raw["body"]["text"]), 80)
                    .replace(['\n', '\t'], " ");
                format!("{author}: {excerpt}")
            })
            .unwrap_or_else(|| "Earlier message".into())
    }
    fn compose(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let content_width = window.viewport_size().width
            - if self.sidebar_visible(window) {
                window.rem_size() * 16.
            } else {
                px(0.)
            };
        let show_hint = content_width >= window.rem_size() * 45.;
        let completing = !self.mention_candidates(cx).is_empty();
        let mut pane = div()
            .key_context(if completing {
                "ApronComposer MentionCompletion"
            } else {
                "ApronComposer"
            })
            .on_action(cx.listener(|v, _: &PasteImage, w, cx| v.paste_image(w, cx)))
            .on_action(cx.listener(|v, _: &MentionNext, _, cx| {
                v.mention_index = (v.mention_index + 1) % v.mention_candidates(cx).len().max(1);
                cx.notify();
            }))
            .on_action(cx.listener(|v, _: &MentionPrevious, _, cx| {
                let n = v.mention_candidates(cx).len().max(1);
                v.mention_index = (v.mention_index + n - 1) % n;
                cx.notify();
            }))
            .on_action(cx.listener(|v, _: &MentionAccept, w, cx| {
                let list = v.mention_candidates(cx);
                if !list.is_empty() {
                    let id = list[v.mention_index % list.len()].id.clone();
                    v.add_mention(&id, w, cx);
                }
            }))
            .flex()
            .flex_col()
            .gap_2()
            .m_3()
            .mt_2()
            .p_2()
            .rounded_xl()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border);
        if let Some(reply) = self.reply {
            pane = pane.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().accent)
                    .border_l_2()
                    .border_color(cx.theme().primary)
                    .text_sm()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(format!("Replying to {}", self.reply_summary(reply))),
                    )
                    .child(
                        crate::design::button("cancel-reply")
                            .ghost()
                            .xsmall()
                            .label("Cancel reply")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reply = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        pane = pane.child(self.staged_images_view(cx));
        if self.transfer_busy {
            pane = pane.child(
                crate::design::button("cancel-transfer")
                    .ghost()
                    .small()
                    .label("Cancel transfer")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.command(Command::CancelTransfer, cx);
                    })),
            )
        }
        pane = pane.child(
            Textarea::new(&self.composer)
                .appearance(false)
                .aria_label("Message")
                .accessibility_id("composer")
                .w_full(),
        );
        pane = pane.child(self.mention_list(cx));
        pane.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    crate::design::button("attach")
                        .flex_shrink_0()
                        .ghost()
                        .small()
                        .icon(IconName::Plus)
                        .accessibility_label("Attach files")
                        .tooltip("Attach files…")
                        .disabled(
                            !self.ready
                                || self.pending
                                || self.transfer_busy
                                || !self.caps.iter().any(|c| c == "embed:upload"),
                        )
                        .on_click(cx.listener(|this, _, window, cx| this.attach(window, cx))),
                )
                .child(
                    crate::design::button("mention-person")
                        .ghost()
                        .small()
                        .label("@")
                        .accessibility_label("Mention a person")
                        .tooltip("Mention a person")
                        .on_click(cx.listener(|v, _, w, cx| {
                            v.composer.update(cx, |s, cx| {
                                s.replace("@", w, cx);
                                s.focus(w, cx);
                            });
                            cx.notify();
                        })),
                )
                .child(
                    crate::design::button("preview-markdown")
                        .ghost()
                        .small()
                        .icon(IconName::Eye)
                        .accessibility_label("Preview Markdown")
                        .tooltip("Preview Markdown…")
                        .on_click(cx.listener(|v, _, w, cx| v.preview_markdown(w, cx))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .when(show_hint, |d| {
                            d.child("Markdown supported  ·  Ctrl+Enter to send")
                        }),
                )
                .child(
                    crate::design::button("send")
                        .flex_shrink_0()
                        .primary()
                        .label(if self.pending {
                            "Sending…"
                        } else if !self.ready || self.joined() {
                            "Send"
                        } else {
                            "Join room"
                        })
                        .disabled(!self.ready || self.pending)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.joined() {
                                this.send(window, cx);
                            } else {
                                this.command(
                                    Command::Join {
                                        room: this.room.clone(),
                                    },
                                    cx,
                                );
                            }
                        })),
                ),
        )
        .into_any_element()
    }
}
impl Render for Conversation {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::design::apply(cx);
        let started = std::time::Instant::now();
        if self.inspect {
            self.geometry.borrow_mut().clear();
        }
        let rows = self.rows.clone();
        let owner = cx.weak_entity();
        let mut content = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(self.header(window, cx))
            .when(self.searching, |d| d.child(self.search_bar(cx)));
        if !self.error.is_empty() {
            content = content.child(
                div()
                    .px_6()
                    .py_2()
                    .text_sm()
                    .bg(cx.theme().danger.opacity(0.08))
                    .text_color(cx.theme().danger)
                    .child(model::clean(&self.error, 500)),
            );
        }
        if !self.transfer.is_empty() {
            content = content.child(
                div()
                    .px_6()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(model::clean(&self.transfer, 500)),
            );
        }
        let scroller = MessageScroller::new(
            "transcript",
            self.scroller.clone(),
            move |ix, window, cx| {
                if let Some(m) = rows.get(ix) {
                    if let Some(e) = owner.upgrade() {
                        e.read(cx)
                            .rendered_rows
                            .set(e.read(cx).rendered_rows.get() + 1);
                    }
                    Self::row(
                        m.clone(),
                        ix > 0
                            && rows
                                .get(ix - 1)
                                .is_some_and(|prev| apron_gpui::model::grouped(prev, m)),
                        owner.clone(),
                        window,
                        cx,
                    )
                } else {
                    div().into_any_element()
                }
            },
        )
        .with_list_style(StyleRefinement::default().px_3().py_3())
        .with_row_style(StyleRefinement::default().pb_1())
        .with_jump_button_renderer(|b| b.label("Latest").small())
        .with_jump_button_transition(Duration::ZERO);
        let transcript = if self.rows.is_empty() {
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(
                    div()
                        .size_12()
                        .rounded_xl()
                        .bg(cx.theme().muted)
                        .text_color(cx.theme().primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_2xl()
                        .child("#"),
                )
                .child(if self.loading {
                    "Loading conversation…"
                } else if self.status == "Sign-in required" {
                    "Sign in to continue"
                } else if !self.ready {
                    "Waiting for connection"
                } else {
                    "No messages yet"
                })
                .child(if self.ready {
                    "Choose a room or start a conversation below."
                } else if self.status == "Sign-in required" {
                    "Use Sign in above to enter a token or choose guest access."
                } else {
                    "Drafts stay local while the connection is unavailable."
                })
                .into_any_element()
        } else {
            scroller.into_any_element()
        };
        let transcript_bounds = self.transcript_bounds.clone();
        content = content
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        canvas(
                            move |bounds, _, _| *transcript_bounds.borrow_mut() = bounds,
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(transcript),
            )
            .child(self.compose(window, cx));
        let shell = div()
            .key_context("Apron")
            .size_full()
            .flex()
            .items_stretch()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .on_action(cx.listener(|v,_:&FindMessages,w,cx|v.open_search(w,cx)))
            .on_action(cx.listener(|v,_:&NextMatch,_,cx|v.step_match(true,cx)))
            .on_action(cx.listener(|v,_:&PreviousMatch,_,cx|v.step_match(false,cx)))
            .on_action(cx.listener(|_,_:&ToggleTheme,w,cx| {use gpui_kit::component::{Theme,ThemeMode};let mode=if cx.theme().is_dark(){ThemeMode::Light}else{ThemeMode::Dark};Theme::change(mode,Some(w),cx);crate::design::apply(cx);}))
            .on_action(cx.listener(|v,_:&CreateRoom,w,cx|v.manage_dialog(apron_gpui::workflows::Kind::Create,w,cx)))
            .on_action(cx.listener(|v,_:&CreateThread,w,cx|v.manage_dialog(apron_gpui::workflows::Kind::Thread,w,cx)))
            .on_action(cx.listener(|v,_:&RoomSettings,w,cx|v.manage_dialog(apron_gpui::workflows::Kind::Settings,w,cx)))
            .on_action(cx.listener(|v,_:&AddMember,w,cx|v.manage_dialog(apron_gpui::workflows::Kind::AddMember,w,cx)))
            .on_action(cx.listener(|v,_:&RemoveMember,w,cx|v.manage_dialog(apron_gpui::workflows::Kind::RemoveMember,w,cx)))
            .on_action(cx.listener(|v,_:&LeaveRoom,w,cx|v.manage_dialog(apron_gpui::workflows::Kind::Leave,w,cx)))
            .on_action(cx.listener(|v,_:&ShowMembers,w,cx|v.member_dialog(w,cx)))
            .on_action(cx.listener(|v,_:&ToggleNotifications,_,cx|{v.notifications = !v.notifications;cx.notify();}))
            .on_action(cx.listener(|v,_:&ToggleMute,_,cx|v.set_status(serde_json::json!({"room_id":v.room,"mute":!v.muted(&v.room)}),"status",cx)))
            .on_action(cx.listener(|v,_:&PresenceOnline,_,cx|v.set_status(serde_json::json!({"status":"online"}),"me",cx)))
            .on_action(cx.listener(|v,_:&PresenceNone,_,cx|v.set_status(serde_json::json!({"status":""}),"me",cx)))
            .on_action(cx.listener(|_, _: &ShowGallery, w, cx| crate::design::gallery(w,cx)))
            .on_action(cx.listener(|this, _: &SendMessage, window, cx| this.send(window, cx)))
            .on_action(cx.listener(|this, _: &FindRoom, window, cx| {
                this.open_switcher(window, cx);
            }))
            .on_action(cx.listener(|v,_:&NextMention,_,cx|{
                if let Some(id)=v.attention.first_mention(&v.room){
                    if let Some(ix)=v.rows.iter().position(|m|m.id==id){v.scroller.update(cx,|s,cx|s.scroll_to_item(ix,cx));}
                    else {v.error="This unread mention is outside retained history. Load earlier messages to inspect it.".into();cx.notify();}
                }
            }))
            .on_action(cx.listener(|v,_:&OpenEmail,w,cx|v.email_dialog(w,cx)))
            .on_action(cx.listener(|this, _: &OpenPrivacy, w, cx| this.privacy_dialog(w, cx)))
            .on_action(cx.listener(|this, _: &MarkRead, _, cx| this.mark_read(cx)))
            .on_action(
                cx.listener(|this, _: &OpenSession, window, cx| this.session_dialog(window, cx)),
            )
            .on_action(cx.listener(|_, _: &ShowHelp, window, cx| Self::help_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &LoadOlder, _, cx| this.older(cx)))
            .on_action(cx.listener(|this, _: &Latest, _, cx| {
                this.scroller.update(cx, |s, cx| s.scroll_to_end(cx))
            }))
            .on_action(cx.listener(|this, _: &CancelReply, w, cx| {
                if this.searching && this.search.focus_handle(cx).is_focused(w) {
                    this.searching=false;this.composer.update(cx,|s,cx|s.focus(w,cx));
                }else{this.reply = None;}
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &ToggleRooms, window, cx| this.toggle_rooms(window, cx)),
            )
            .when(self.sidebar_visible(window), |d| {
                d.child(self.navigation(cx))
            })
            .child(content);
        self.max_render_us = self.max_render_us.max(started.elapsed().as_micros());
        shell
    }
}
