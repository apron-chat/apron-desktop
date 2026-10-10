use apron_gpui::{
    attention::Attention,
    auth::Secret,
    media::Attachment,
    model::{self, Message, Store},
    transport::{Bridge, Command, Event},
};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Selectable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
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
        FindRoom,
        MarkRead,
        OpenSession,
        ShowHelp
    ]
);
pub struct Conversation {
    pub(crate) store: Store,
    pub(crate) rows: Rc<Vec<Arc<Message>>>,
    pub(crate) scroller: Entity<MessageScrollerState>,
    pub(crate) composer: Entity<TextareaState>,
    bridge: Bridge,
    server: String,
    pub(crate) authenticated: bool,
    pub(crate) attention: Attention,
    pub(crate) filter: Entity<InputState>,
    pub(crate) session_form: Option<WeakEntity<crate::session::SessionForm>>,
    rooms: BTreeMap<String, Value>,
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
    caps: Vec<String>,
    status: String,
    pub(crate) error: String,
    transfer: String,
    transfer_busy: bool,
    sidebar: Option<bool>,
    open_message: Option<u64>,
    conversation_menu_open: bool,
    pub(crate) rendered_rows: std::cell::Cell<usize>,
    pub(crate) max_render_us: u128,
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
                .auto_grow(2, 5)
        });
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a room…"));
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
            cx.notify();
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
            server: url,
            authenticated: false,
            attention: Attention::default(),
            filter,
            session_form: None,
            rooms: BTreeMap::new(),
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
            caps: Vec::new(),
            status: "Connecting…".into(),
            error: String::new(),
            transfer: String::new(),
            transfer_busy: false,
            sidebar: None,
            open_message: None,
            conversation_menu_open: false,
            rendered_rows: std::cell::Cell::new(0),
            max_render_us: 0,
            geometry: Rc::new(std::cell::RefCell::new(BTreeMap::new())),
            inspect: false,
            _events: task,
            _subscriptions: vec![subscription, observation, activation, filter_subscription],
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
    fn read_visible(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.ready
            && !self.loading
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
            cx.notify();
        }
    }
    pub(crate) fn change_session(
        &mut self,
        token: Option<Secret>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending || self.transfer_busy {
            self.error =
                "Finish the current send or cancel the transfer before changing sessions.".into();
            cx.notify();
            return;
        }
        let bridge = match Bridge::start_with_token(self.server.clone(), token) {
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
        self.files.clear();
        self.reply = None;
        self.you.clear();
        self.composer
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.scroller.update(cx, |s, cx| s.reset(0, cx));
        cx.notify();
    }
    pub(crate) fn can_change_session(&self) -> bool {
        !self.pending && !self.transfer_busy
    }
    pub(crate) fn session_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        let server = self.server.clone();
        let form = cx.new(|cx| crate::session::SessionForm::new(owner, server, window, cx));
        self.session_form = Some(form.downgrade());
        window.open_dialog(cx, move |dialog, _, _| {
            let clear = form.clone();
            dialog
                .title("Sign in to Apron")
                .child(form.clone())
                .on_close(move |_, window, cx| clear.update(cx, |f, cx| f.clear(window, cx)))
        });
    }
    fn help_dialog(window: &mut Window, cx: &mut App) {
        window.open_dialog(cx,|dialog,_,_|dialog.title("Keyboard & message actions").child(div().flex().flex_col().gap_3()
            .child("Ctrl+Enter — send · Enter — new line")
            .child("Ctrl+K — find a room · Ctrl+B — toggle navigation")
            .child("Ctrl+PageUp — earlier history · Ctrl+PageDown — latest")
            .child("Right-click a message, or Tab to its actions button and press Enter")
            .child("With a message focused: R — reply · Ctrl+Shift+C — copy message")
            .child("Ctrl+Shift+U — mark this room read")
            .child("Ctrl+C — copy selected text · Escape — close menu or cancel reply")
            .child("Unread counts cover messages received during this session. Mention badges use server-provided mention IDs, not matching text.")));
    }
    fn listing(&mut self, list: &Value) {
        model::update_rooms(&mut self.rooms, list);
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
        self.room = room;
        self.generation += 1;
        self.loading = false;
        self.pending = false;
        self.files.clear();
        self.transfer.clear();
        self.before = None;
        self.more = false;
        let (text, reply) = self.drafts.get(&self.room).cloned().unwrap_or_default();
        self.reply = reply;
        self.composer
            .update(cx, |s, cx| s.set_value(text, window, cx));
        self.rows = Rc::new(self.store.rows(&self.room));
        self.scroller
            .update(cx, |s, cx| s.reset(self.rows.len(), cx));
        self.history(None, cx);
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
        let changed: Vec<usize> = next
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                self.rows
                    .get(*i)
                    .is_some_and(|old| old.id == m.id && old.log != m.log)
            })
            .map(|(i, _)| i)
            .collect();
        self.rows = Rc::new(next);
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
            Event::State(s) => {
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
                self.rooms.clear();
                self.listing(&rooms);
                self.caps = caps;
                self.ready = true;
                self.status = "Connected".into();
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
            }
            Event::Notice(frame) => match frame["method"].as_str() {
                Some("message") => {
                    let raw = &frame["params"];
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
                                self.attention.receive(
                                    &message.room,
                                    message.id,
                                    raw["from"]["user_id"] == self.you,
                                    mentioned,
                                    old.is_some() && mentioned && !previously,
                                );
                            }
                        }
                        self.refresh(cx);
                        self.read_visible(window, cx);
                    }
                }
                Some("room_update") => self.listing(&frame["params"]),
                Some("activity") if frame["params"]["from"]["user_id"] == self.you => {
                    if let (Some(room), Some(id)) = (
                        frame["params"]["room_id"].as_str(),
                        model::number(&frame["params"]["read_message_id"]),
                    ) {
                        self.attention.mark_read(room, id);
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
                self.refresh(cx);
                self.read_visible(window, cx);
            }
            Event::Sent {
                generation,
                text,
                reply,
            } => {
                if generation == self.generation {
                    self.pending = false;
                    self.files.clear();
                    if self.composer.read(cx).value().as_ref() == text && self.reply == reply {
                        self.composer
                            .update(cx, |s, cx| s.set_value("", window, cx));
                        self.reply = None;
                    }
                    self.transfer.clear();
                }
            }
            Event::Files { generation, files } => {
                if generation == self.generation {
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
                text,
                reply: self.reply,
                files: self.files.clone(),
            },
            cx,
        ) {
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
    fn row(m: Arc<Message>, owner: WeakEntity<Self>, cx: &mut App) -> AnyElement {
        let id = m.id;
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
            .key_context("Message")
            .on_action(move |_: &ReplyMessage, window, cx| {
                let _ = reply_owner.update(cx, |v, cx| v.begin_reply(id, window, cx));
            })
            .on_action(move |_: &CopyMessage, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
            })
            .when(mentioned, |d| d.bg(cx.theme().primary.opacity(0.05)))
            .flex()
            .flex_col()
            .gap_1()
            .min_w_0()
            .py_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .max_w_48()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(timestamp),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new(("message-actions", id))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Ellipsis)
                            .accessibility_label("Message actions")
                            .when(!menu_open, |b| {
                                b.tooltip("Message actions · Tab then Enter")
                            })
                            .opacity(if menu_open { 1. } else { 0. })
                            .group_hover(group, |s| s.opacity(1.))
                            .focus_visible(|s| s.opacity(1.))
                            .dropdown_menu(move |menu, _, cx| {
                                Self::message_menu(menu, &menu_owner, cx)
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
                    row = row.child(
                        Button::new(SharedString::from(format!("save-{id}-{ix}")))
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
        row.context_menu(move |menu, _, cx| Self::message_menu(menu, &context_owner, cx))
            .into_any_element()
    }
    fn message_menu(menu: PopupMenu, owner: &WeakEntity<Self>, cx: &App) -> PopupMenu {
        let can_reply = owner
            .upgrade()
            .is_some_and(|v| v.read(cx).ready && v.read(cx).joined());
        menu.menu_with_disabled("Reply", Box::new(ReplyMessage), !can_reply)
            .menu("Copy message", Box::new(CopyMessage))
    }
    fn navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut pane = div()
            .w_56()
            .min_w_48()
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div().h_16().px_4().flex().items_center().child(
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
                    .child("Rooms & threads"),
            );
        let mut rooms = div()
            .id("rooms")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2();
        let query = self.filter.read(cx).value().to_lowercase();
        pane = pane.child(
            div()
                .px_3()
                .pb_3()
                .child(Input::new(&self.filter).aria_label("Find a room").w_full()),
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
            if !format!("{} {}", id, room["title"].as_str().unwrap_or(""))
                .to_lowercase()
                .contains(&query)
            {
                continue;
            }
            matches += 1;
            let target = id.clone();
            let title = model::clean(room["title"].as_str().unwrap_or(id), 80);
            let (unread, mentions) = self.attention.counts(id);
            rooms = rooms.child(
                Button::new(SharedString::from(format!("room-{id}")))
                    .ghost()
                    .selected(id == &self.room)
                    .w_full()
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
                                d.child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(
                                    if mentions > 0 {
                                        format!("{unread} · @{mentions}")
                                    } else {
                                        unread.to_string()
                                    },
                                ))
                            }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select(target.clone(), window, cx)
                    })),
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
                        "No rooms available"
                    } else {
                        "Rooms appear after connecting"
                    }),
            );
        }
        pane = pane.child(rooms).child(
            div()
                .p_4()
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
            .unwrap_or(window.viewport_size().width >= px(900.))
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
                window.rem_size() * 14.
            } else {
                px(0.)
            };
        let show_history = content_width >= window.rem_size() * 32.;
        let can_load = self.more && !self.loading && self.ready;
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
                Button::new("toggle-rooms")
                    .ghost()
                    .small()
                    .label("Rooms")
                    .selected(self.sidebar_visible(window))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_rooms(window, cx))),
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
                            .child(self.status.clone()),
                    ),
            )
            .when(show_history, |d| {
                d.child(
                    Button::new("older")
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
                    Button::new("unread")
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
                    Button::new("reconnect")
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
                Button::new("conversation-menu")
                    .ghost()
                    .small()
                    .icon(IconName::Ellipsis)
                    .accessibility_label("Conversation and session actions")
                    .when(!self.conversation_menu_open, |b| {
                        b.tooltip("Conversation and session actions")
                    })
                    .dropdown_menu(move |menu, _, _| {
                        menu.menu("Mark room read", Box::new(MarkRead))
                            .menu("Jump to latest", Box::new(Latest))
                            .menu_with_disabled("Load earlier", Box::new(LoadOlder), !can_load)
                            .separator()
                            .menu("Sign in with token…", Box::new(OpenSession))
                            .menu("Keyboard shortcuts…", Box::new(ShowHelp))
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
                window.rem_size() * 14.
            } else {
                px(0.)
            };
        let show_hint = content_width >= window.rem_size() * 26.;
        let mut pane = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_6()
            .py_4()
            .border_t_1()
            .border_color(cx.theme().border);
        if let Some(reply) = self.reply {
            pane = pane.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(format!("Replying to {}", self.reply_summary(reply))),
                    )
                    .child(
                        Button::new("cancel-reply")
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
        if !self.files.is_empty() {
            let count = self.files.len();
            let bytes = self.files.iter().map(|f| f.bytes.len()).sum::<usize>();
            pane = pane.child(
                div()
                    .flex()
                    .gap_2()
                    .text_sm()
                    .child(format!(
                        "{count} files · {bytes} bytes: {}",
                        self.files
                            .iter()
                            .map(|f| f.title.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .child(
                        Button::new("clear-files")
                            .ghost()
                            .small()
                            .label("Remove")
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.files.clear();
                                cx.notify();
                            })),
                    ),
            );
        }
        if self.transfer_busy {
            pane = pane.child(
                Button::new("cancel-transfer")
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
                .aria_label("Message")
                .accessibility_id("composer")
                .w_full(),
        );
        pane.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("attach")
                        .flex_shrink_0()
                        .ghost()
                        .small()
                        .label("Attach files…")
                        .disabled(
                            !self.ready
                                || self.pending
                                || self.transfer_busy
                                || !self.caps.iter().any(|c| c == "embed:upload"),
                        )
                        .on_click(cx.listener(|this, _, window, cx| this.attach(window, cx))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .when(show_hint, |d| {
                            d.child("Ctrl+Enter to send · text stays local until sent")
                        }),
                )
                .child(
                    Button::new("send")
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
            .child(self.header(window, cx));
        if !self.error.is_empty() {
            content = content.child(
                div()
                    .px_6()
                    .py_2()
                    .text_sm()
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
        let scroller =
            MessageScroller::new("transcript", self.scroller.clone(), move |ix, _, cx| {
                if let Some(m) = rows.get(ix) {
                    if let Some(e) = owner.upgrade() {
                        e.read(cx)
                            .rendered_rows
                            .set(e.read(cx).rendered_rows.get() + 1);
                    }
                    Self::row(m.clone(), owner.clone(), cx)
                } else {
                    div().into_any_element()
                }
            })
            .with_list_style(StyleRefinement::default().px_6().py_3())
            .with_row_style(StyleRefinement::default().pb_2())
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
        content = content
            .child(div().flex_1().min_h_0().child(transcript))
            .child(self.compose(window, cx));
        let shell = div()
            .key_context("Apron")
            .size_full()
            .flex()
            .items_stretch()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .on_action(cx.listener(|this, _: &SendMessage, window, cx| this.send(window, cx)))
            .on_action(cx.listener(|this, _: &FindRoom, window, cx| {
                this.sidebar = Some(true);
                this.filter.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MarkRead, _, cx| this.mark_read(cx)))
            .on_action(
                cx.listener(|this, _: &OpenSession, window, cx| this.session_dialog(window, cx)),
            )
            .on_action(cx.listener(|_, _: &ShowHelp, window, cx| Self::help_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &LoadOlder, _, cx| this.older(cx)))
            .on_action(cx.listener(|this, _: &Latest, _, cx| {
                this.scroller.update(cx, |s, cx| s.scroll_to_end(cx))
            }))
            .on_action(cx.listener(|this, _: &CancelReply, _, cx| {
                this.reply = None;
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
