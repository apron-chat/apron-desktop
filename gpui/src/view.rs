use apron_gpui::{
    media::Attachment,
    model::{self, Message, Store},
    transport::{Bridge, Command, Event},
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{InputEvent, Textarea, TextareaState},
    message_scroller::{MessageScroller, MessageScrollerState},
    text::TextView,
};
use gpui_kit::{prelude::*, *};
use serde_json::Value;
use std::{collections::BTreeMap, rc::Rc, sync::Arc, time::Duration};

actions!(
    apron,
    [SendMessage, LoadOlder, Latest, CancelReply, ToggleRooms]
);
pub struct Conversation {
    pub(crate) store: Store,
    pub(crate) rows: Rc<Vec<Arc<Message>>>,
    pub(crate) scroller: Entity<MessageScrollerState>,
    pub(crate) composer: Entity<TextareaState>,
    bridge: Bridge,
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
    pub(crate) rendered_rows: std::cell::Cell<usize>,
    pub(crate) max_render_us: u128,
    pub(crate) geometry: Rc<std::cell::RefCell<BTreeMap<u64, Bounds<Pixels>>>>,
    pub(crate) inspect: bool,
    _events: Task<()>,
    _subscriptions: Vec<Subscription>,
}
impl Conversation {
    pub fn new(url: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let bridge = Bridge::start(url).expect("validated server address");
        let events = bridge.events.clone();
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Write a message…")
                .auto_grow(2, 5)
        });
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
        let observation = cx.observe(&scroller, |_, _, cx| cx.notify());
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
            rendered_rows: std::cell::Cell::new(0),
            max_render_us: 0,
            geometry: Rc::new(std::cell::RefCell::new(BTreeMap::new())),
            inspect: false,
            _events: task,
            _subscriptions: vec![subscription, observation],
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
    fn listing(&mut self, list: &Value) {
        for key in ["joined", "not_joined"] {
            if let Some(rooms) = list[key].as_array() {
                for room in rooms.iter().take(256) {
                    if let Some(id) = room["room_id"].as_str() {
                        let mut room = room.clone();
                        room["_joined"] = Value::Bool(key == "joined");
                        self.rooms.insert(id.to_owned(), room);
                    }
                }
            }
        }
        while self.rooms.len() > 256 {
            self.rooms.pop_last();
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
            Event::Ready { you, rooms, caps } => {
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
                    if self.store.apply(frame["params"].clone()) {
                        self.refresh(cx);
                    }
                }
                Some("room_update") => self.listing(&frame["params"]),
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
                    self.transfer = "Message accepted".into();
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
    pub(crate) fn smoke_reply(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
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
        let mut row = div()
            .id(("message", id))
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
                        Button::new(("reply", id))
                            .ghost()
                            .xsmall()
                            .label("Reply")
                            .on_click(move |_, window, cx| {
                                let _ = reply_owner.update(cx, |this, cx| {
                                    this.reply = Some(id);
                                    this.composer.update(cx, |s, cx| s.focus(window, cx));
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        Button::new(("copy", id))
                            .ghost()
                            .xsmall()
                            .label("Copy")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
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
        row.into_any_element()
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
        for (id, room) in &self.rooms {
            let target = id.clone();
            let title = model::clean(room["title"].as_str().unwrap_or(id), 80);
            rooms = rooms.child(
                Button::new(SharedString::from(format!("room-{id}")))
                    .ghost()
                    .selected(id == &self.room)
                    .w_full()
                    .justify_start()
                    .label(format!(
                        "{} {title}",
                        if room["parent_room_id"].is_string() {
                            "↳"
                        } else {
                            "#"
                        }
                    ))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select(target.clone(), window, cx)
                    })),
            );
        }
        pane = pane.child(rooms).child(
            div()
                .p_4()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Guest session"),
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
            .child(
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
            .child(
                Button::new("latest")
                    .ghost()
                    .small()
                    .label("Latest")
                    .disabled(!self.ready)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.history(None, cx);
                        this.scroller.update(cx, |s, cx| s.scroll_to_end(cx));
                    })),
            )
            .child(
                Button::new("reconnect")
                    .ghost()
                    .small()
                    .label("Reconnect")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.command(Command::Reconnect, cx);
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
    fn compose(&self, cx: &mut Context<Self>) -> AnyElement {
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
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Ctrl+Enter to send · text stays local until sent"),
                )
                .child(
                    Button::new("send")
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
                } else if !self.ready {
                    "Waiting for connection"
                } else {
                    "No messages yet"
                })
                .child(if self.ready {
                    "Choose a room or start a conversation below."
                } else {
                    "Drafts stay local while the connection is unavailable."
                })
                .into_any_element()
        } else {
            scroller.into_any_element()
        };
        content = content
            .child(div().flex_1().min_h_0().child(transcript))
            .child(self.compose(cx));
        let shell = div()
            .key_context("Apron")
            .size_full()
            .flex()
            .items_stretch()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .on_action(cx.listener(|this, _: &SendMessage, window, cx| this.send(window, cx)))
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
