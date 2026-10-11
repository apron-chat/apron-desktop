use super::*;
use apron_gpui::workflows::Kind;
pub struct ManageForm {
    owner: WeakEntity<Conversation>,
    kind: Kind,
    room: Value,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    user: Entity<InputState>,
    pending: Option<u64>,
    error: String,
    uncertain: bool,
}
impl ManageForm {
    pub fn finish(
        &mut self,
        op: u64,
        result: Result<Value, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending != Some(op) {
            return;
        }
        self.pending = None;
        match result {
            Ok(_) => w.close_dialog(cx),
            Err(e) => {
                self.uncertain = e.contains("uncertain");
                self.error = e;
                cx.notify();
            }
        }
    }
    fn submit(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() || self.uncertain {
            return;
        }
        let request = self.kind.request(
            &self.room,
            &self.title.read(cx).value(),
            &self.description.read(cx).value(),
            &self.user.read(cx).value(),
        );
        let (method, payload) = match request {
            Ok(v) => v,
            Err(e) => {
                self.error = e.to_string();
                cx.notify();
                return;
            }
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let result = owner.update(cx, |v, cx| {
            if !v.ready || !v.caps.iter().any(|c| c == "rooms") {
                return None;
            }
            if self.kind == Kind::Settings
                && v.rooms
                    .get(model::text(&self.room["room_id"]))
                    .is_some_and(|r| r["log_id"] != self.room["log_id"])
            {
                return None;
            }
            v.mutation_serial += 1;
            let op = v.mutation_serial;
            let sent = v.command(
                Command::Manage {
                    operation: op,
                    method: method.into(),
                    payload,
                },
                cx,
            );
            if sent {
                v.manage_pending = Some(op);
                Some(op)
            } else {
                None
            }
        });
        if let Some(op) = result {
            self.pending = Some(op);
            self.error.clear();
        } else {
            self.error = "Room or connection changed. Close and reopen this form.".into();
        }
        let _ = w;
        cx.notify();
    }
}
impl Render for ManageForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut pane = crate::design::surface(cx);
        if self.kind.room_form() {
            pane = pane
                .child("Title")
                .child(Input::new(&self.title).aria_label("Room title").w_full())
                .child("Description")
                .child(
                    Textarea::new(&self.description)
                        .aria_label("Room description")
                        .w_full(),
                );
        } else if self.kind != Kind::Leave {
            pane = pane
                .child("Exact user ID")
                .child(Input::new(&self.user).aria_label("Exact user ID").w_full())
                .child(
                    div()
                        .text_sm()
                        .child("The server decides whether you can change this membership."),
                );
        } else {
            pane = pane.child("Leave this room? You can join again if server policy allows it.");
        }
        pane.when(!self.error.is_empty(), |d| {
            d.child(
                div()
                    .text_color(cx.theme().danger)
                    .child(self.error.clone()),
            )
        })
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    crate::design::button("manage-cancel")
                        .label("Cancel")
                        .on_click(|_, w, cx| w.close_dialog(cx)),
                )
                .child(
                    crate::design::button("manage-save")
                        .primary()
                        .label(if self.pending.is_some() {
                            "Saving…"
                        } else {
                            self.kind.title()
                        })
                        .disabled(self.pending.is_some() || self.uncertain)
                        .on_click(cx.listener(|v, _, w, cx| v.submit(w, cx))),
                ),
        )
    }
}
impl Conversation {
    pub(crate) fn manage_dialog(&mut self, kind: Kind, w: &mut Window, cx: &mut Context<Self>) {
        if w.has_active_dialog(cx) {
            return;
        }
        if !self.ready || !self.caps.iter().any(|c| c == "rooms") {
            self.error = "This connection does not support room management.".into();
            cx.notify();
            return;
        }
        let room = self.rooms.get(&self.room).cloned().unwrap_or_default();
        let owner = cx.weak_entity();
        let restore = owner.clone();
        let form = cx.new(|cx| {
            let title = cx.new(|cx| InputState::new(w, cx).placeholder("Room title"));
            let description = cx.new(|cx| TextareaState::new(w, cx).auto_grow(2, 4));
            let user = cx.new(|cx| InputState::new(w, cx).placeholder("user_id"));
            if kind == Kind::Settings {
                title.update(cx, |s, cx| {
                    s.set_value(model::clean(model::text(&room["title"]), 200), w, cx)
                });
                description.update(cx, |s, cx| {
                    s.set_value(model::clean(model::text(&room["description"]), 4000), w, cx)
                });
            }
            if kind.room_form() {
                title.update(cx, |s, cx| s.focus(w, cx));
            } else {
                user.update(cx, |s, cx| s.focus(w, cx));
            }
            ManageForm {
                owner,
                kind,
                room,
                title,
                description,
                user,
                pending: None,
                error: String::new(),
                uncertain: false,
            }
        });
        self.manage_form = Some(form.downgrade());
        w.open_dialog(cx, move |d, _, _| {
            let restore = restore.clone();
            d.title(kind.title())
                .child(form.clone())
                .on_close(move |_, w, cx| {
                    let _ =
                        restore.update(cx, |v, cx| v.composer.update(cx, |s, cx| s.focus(w, cx)));
                })
        });
    }
    pub(crate) fn member_dialog(&self, w: &mut Window, cx: &mut Context<Self>) {
        if w.has_active_dialog(cx) {
            return;
        }
        let room = self.rooms.get(&self.room).cloned().unwrap_or_default();
        let members = room["members"].as_array().cloned().unwrap_or_default();
        let total = room["member_count"]
            .as_u64()
            .unwrap_or(members.len() as u64);
        let rows: Vec<String> = members
            .iter()
            .take(512)
            .map(|member| {
                let id = model::text(&member["user_id"]);
                let user = self.users.get(id).unwrap_or(member);
                format!(
                    "{} · {} · {}",
                    model::clean(user["name"].as_str().unwrap_or(id), 80),
                    model::clean(id, 128),
                    model::clean(user["status"].as_str().unwrap_or("unknown presence"), 40)
                )
            })
            .collect();
        w.open_dialog(cx, move |d, _, _| {
            d.title(format!("Members · {} shown / {total}", rows.len()))
                .child(
                    div()
                        .id("members-list")
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(rows.iter().cloned().map(|row| div().p_2().child(row))),
                )
        });
    }
}
