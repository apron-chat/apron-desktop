use super::*;
use std::time::Instant;
impl Conversation {
    pub(crate) fn record_input(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.last_input = Instant::now();
        if self.idle_reported == Some(false) && self.idle_timer.is_some() && w.is_window_active() {
            return;
        }
        self.update_idle(w, cx);
    }
    pub(crate) fn update_idle(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.idle_timer = None;
        if !self.ready || !self.caps.iter().any(|c| c == "status") {
            return;
        }
        if !self.preferences.report_idle && self.idle_reported != Some(true) {
            self.idle_reported = None;
            return;
        }
        let elapsed = self.last_input.elapsed().as_secs();
        let desired =
            apron_gpui::presence::idle(self.preferences.report_idle, w.is_window_active(), elapsed);
        let idle = desired.is_some();
        if self.idle_reported != Some(idle) {
            let value = desired
                .map(|s| serde_json::json!(s))
                .unwrap_or(serde_json::json!(false));
            self.mutation_serial += 1;
            if self.command(
                Command::Manage {
                    operation: self.mutation_serial,
                    method: "status".into(),
                    payload: serde_json::json!({"idle":value}),
                },
                cx,
            ) {
                self.idle_reported = Some(idle);
            }
        }
        if self.preferences.report_idle && w.is_window_active() && !idle {
            self.idle_timer = Some(cx.spawn_in(w, async move |v, cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(300 - elapsed.min(299)))
                    .await;
                let _ = v.update_in(cx, |v, w, cx| v.update_idle(w, cx));
            }));
        }
    }

    pub(crate) fn activity_changed(&mut self, cx: &mut Context<Self>) {
        if self.preferences.suppress_typing
            || !self.ready
            || !self.joined()
            || !self.caps.iter().any(|c| c == "activity")
        {
            return;
        }
        if self
            .last_typing
            .is_some_and(|t| t.elapsed() < Duration::from_secs(4))
        {
            return;
        }
        let typing = if self.composer.read(cx).value().is_empty() {
            0
        } else {
            8
        };
        let _ = self.bridge.commands.try_send(Command::Typing {
            room: self.room.clone(),
            seconds: typing,
        });
        self.last_typing = Some(Instant::now());
    }
    pub(crate) fn stop_typing(&mut self) {
        if self.last_typing.take().is_some() {
            let _ = self.bridge.commands.try_send(Command::Typing {
                room: self.room.clone(),
                seconds: 0,
            });
        }
    }
    pub(crate) fn receive_activity(&mut self, p: &Value, cx: &mut Context<Self>) {
        let room = model::text(&p["room_id"]);
        let user = model::text(&p["from"]["user_id"]);
        if user == self.you || room.len() > 256 || user.len() > 128 || user.is_empty() {
            return;
        }
        let Some(seconds) = p["typing"].as_u64() else {
            return;
        };
        let key = (room.to_owned(), user.to_owned());
        if seconds == 0 {
            self.typing.remove(&key);
        } else if self.typing.len() < 128 || self.typing.contains_key(&key) {
            self.typing
                .insert(key, Instant::now() + Duration::from_secs(seconds.min(30)));
        }
        self.expire_typing(cx);
    }
    fn expire_typing(&mut self, cx: &mut Context<Self>) {
        self.typing.retain(|_, until| *until > Instant::now());
        let next = self.typing.values().min().copied();
        self.typing_timer = None;
        if let Some(until) = next {
            self.typing_timer = Some(cx.spawn(async move |v, cx| {
                cx.background_executor()
                    .timer(until.saturating_duration_since(Instant::now()))
                    .await;
                let _ = v.update(cx, |v, cx| {
                    v.expire_typing(cx);
                    cx.notify();
                });
            }));
        }
    }
    pub(crate) fn typing_label(&self) -> Option<String> {
        let names: Vec<_> = self
            .typing
            .iter()
            .filter(|((room, _), until)| room == &self.room && **until > Instant::now())
            .take(3)
            .map(|((_, user), _)| {
                model::clean(
                    self.users
                        .get(user)
                        .and_then(|u| u["name"].as_str())
                        .unwrap_or(user),
                    40,
                )
            })
            .collect();
        (!names.is_empty()).then(|| format!("{} typing…", names.join(", ")))
    }
    pub(crate) fn status_notice(&mut self, p: &Value) {
        if let Some(mute) = p.get("mute") {
            if !mute.is_boolean() && !mute.is_u64() {
                return;
            }
            let room = model::text(&p["room_id"]);
            if room.len() > 256 {
                return;
            }
            if mute == false || mute.as_u64() == Some(0) {
                self.mutes.remove(room);
            } else if self.mutes.len() < 257 || self.mutes.contains_key(room) {
                let end = if mute == true {
                    None
                } else {
                    mute.as_u64()
                        .map(|n| Instant::now() + Duration::from_secs(n.min(31_536_000)))
                };
                self.mutes.insert(room.into(), end);
            }
        }
    }
    pub(crate) fn muted(&self, room: &str) -> bool {
        apron_gpui::presence::muted(room, &self.rooms, &self.mutes, Instant::now())
    }
    pub(crate) fn set_status(&mut self, payload: Value, method: &str, cx: &mut Context<Self>) {
        if !self.ready || !self.caps.iter().any(|c| c == "status") {
            self.error = "This server does not support presence and mutes.".into();
            cx.notify();
            return;
        }
        self.mutation_serial += 1;
        self.command(
            Command::Manage {
                operation: self.mutation_serial,
                method: method.into(),
                payload,
            },
            cx,
        );
    }
    pub(crate) fn notify_message(&mut self, room: &str, window: &Window, cx: &mut Context<Self>) {
        if self.own_status == "dnd"
            || !self.notifications
            || self.muted(room)
            || (room == self.room && window.is_window_active())
            || self
                .last_notification
                .is_some_and(|t| t.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        self.last_notification = Some(Instant::now());
        // Deliberately no room title, sender or message body on a lock screen.
        cx.spawn(async move|v,cx|{
            let result=cx.background_executor().spawn(async {notify_rust::Notification::new().summary("Apron").body("New activity in a conversation").appname("Apron").timeout(5000).show().map(|_|()).map_err(|_|"Desktop notification service unavailable. Check your session notification daemon.".to_string())}).await;
            if let Err(e)=result {let _=v.update(cx,|v,cx|{v.error=e;cx.notify();});}
        }).detach();
    }
}
