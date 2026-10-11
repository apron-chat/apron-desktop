use super::*;
use apron_gpui::local_state::{Operation, Preferences, Profile, Snapshot};
pub(crate) enum Preference {
    Theme,
    Notifications,
    Typing,
    Idle,
}
impl Conversation {
    fn apply_preferences(&self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui_kit::component::{Theme, ThemeMode};
        Theme::change(
            if self.preferences.dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        crate::design::apply(cx);
    }
    pub(crate) fn change_preference(
        &mut self,
        preference: Preference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.local_loading || self.profile.is_none() {
            self.error = "Preferences will be available after sign-in completes.".into();
            cx.notify();
            return;
        }
        match preference {
            Preference::Theme => {
                self.preferences.dark = !cx.theme().is_dark();
                self.apply_preferences(window, cx);
            }
            Preference::Notifications => {
                self.notifications = !self.notifications;
                self.preferences.notifications = self.notifications;
            }
            Preference::Typing => {
                self.preferences.suppress_typing = !self.preferences.suppress_typing;
                if self.preferences.suppress_typing {
                    self.stop_typing();
                }
            }
            Preference::Idle => self.preferences.report_idle = !self.preferences.report_idle,
        }
        self.preferences_saved = true;
        self.update_idle(window, cx);
        self.local_save = None;
        self.flush_local(cx);
        cx.notify();
    }
    pub(crate) fn local_snapshot(&self, cx: &App) -> Snapshot {
        let preferences = Some(Preferences {
            save_local: self.local_enabled,
            notifications: self.notifications,
            ..self.preferences.clone()
        });
        if !self.local_enabled {
            return Snapshot {
                version: 1,
                preferences,
                ..Default::default()
            };
        }
        let mut drafts = self.drafts.clone();
        if !self.room.is_empty() {
            drafts.insert(
                self.room.clone(),
                (self.composer.read(cx).value().to_string(), self.reply),
            );
        }
        drafts.retain(|_, (text, reply)| !text.is_empty() || reply.is_some());
        Snapshot {
            version: 1,
            preferences,
            mentions: self
                .draft_mentions
                .iter()
                .filter_map(|(room, ids)| {
                    drafts.get(room).map(|(text, _)| {
                        (room.clone(), apron_gpui::mentions::recipients(text, ids))
                    })
                })
                .collect(),
            drafts,
            reads: self.attention.cursors(),
            attention_evidence: self.attention.evidence(),
            attention_pending: self.attention.pending(),
        }
    }
    pub(crate) fn schedule_local(&mut self, cx: &mut Context<Self>) {
        if !self.local_enabled || self.local_loading {
            return;
        }
        self.local_status = "Saving locally…".into();
        self.local_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(600))
                .await;
            let _ = this.update(cx, |this, cx| this.flush_local(cx));
        }));
    }
    pub(crate) fn flush_local(&mut self, cx: &mut Context<Self>) {
        if (!self.local_enabled && !self.preferences_saved) || self.local_loading {
            return;
        }
        let Some(profile) = self.profile.clone() else {
            return;
        };
        let state = self.local_snapshot(cx);
        match self.local_worker.submit(Operation::Save(profile, state)) {
            Ok(rx) => {
                let epoch = self.local_epoch;
                cx.spawn(async move |this, cx| {
                    let result = rx.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        if this.local_epoch != epoch {
                            return;
                        }
                        match result {
                            Ok(Ok(_)) => {
                                this.local_status = "Saved locally".into();
                                this.local_error.clear();
                            }
                            Ok(Err(e)) => {
                                this.local_error = e;
                                this.local_status = "Not saved".into();
                            }
                            Err(_) => this.local_error = "Local storage stopped".into(),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
            Err(e) => {
                self.local_error = e.to_string();
                cx.notify();
            }
        }
    }
    pub(crate) fn load_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile = Profile::new(&self.server, &self.you);
        if self.profile.as_ref() == Some(&profile) {
            return;
        }
        self.profile = Some(profile.clone());
        self.local_epoch += 1;
        self.local_loading = true;
        self.local_enabled = false;
        self.preferences_saved = false;
        self.preferences = Default::default();
        self.notifications = false;
        self.local_error.clear();
        let epoch = self.local_epoch;
        let result = self.local_worker.submit(Operation::Load(profile));
        let Ok(rx) = result else {
            self.local_loading = false;
            self.local_error = "Could not read local storage".into();
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update_in(cx, |this, w, cx| {
                if epoch != this.local_epoch {
                    return;
                }
                if let Ok(Ok(Some(state))) = &result {
                    this.preferences_saved = state.preferences.is_some();
                    this.preferences = state.preferences.clone().unwrap_or_default();
                    this.local_enabled = state.preferences.as_ref().is_none_or(|p| p.save_local);
                    this.notifications = this.preferences.notifications;
                    if this.preferences_saved {
                        this.apply_preferences(w, cx);
                        this.update_idle(w, cx);
                    }
                    this.attention.restore(&state.reads);
                    this.attention.restore_pending(&state.attention_pending);
                    this.attention
                        .restore_evidence(state.attention_evidence.clone());
                    for (room, draft) in &state.drafts {
                        if !this.local_touched.contains(room) {
                            this.drafts.insert(room.clone(), draft.clone());
                            this.draft_mentions.insert(
                                room.clone(),
                                state
                                    .mentions
                                    .get(room)
                                    .cloned()
                                    .unwrap_or_default()
                                    .into_iter()
                                    .collect(),
                            );
                        }
                    }
                    // Never replace text the user typed while the disk read was outstanding.
                    if this.composer.read(cx).value().is_empty()
                        && !this.local_touched.contains(&this.room)
                        && let Some((text, reply)) = this.drafts.get(&this.room).cloned()
                    {
                        this.reply = reply;
                        this.composer.update(cx, |s, cx| s.set_value(text, w, cx));
                    }
                    this.local_status = "Saved locally".into();
                } else if let Ok(Err(e)) = result {
                    this.local_error = e;
                }
                this.local_loading = false;
                this.recover_attention(cx);
                this.reconcile_attention(w, cx, true);
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn recover_attention(&mut self, cx: &mut Context<Self>) {
        let cursors = self
            .attention
            .cursors()
            .into_iter()
            .filter(|(r, read)| {
                *read > 0 && self.rooms.get(r).is_some_and(|r| r["_joined"] == true)
            })
            .collect();
        self.command(
            Command::Recover {
                epoch: self.local_epoch,
                cursors,
            },
            cx,
        );
    }
    pub(crate) fn reconcile_attention(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        anchor: bool,
    ) {
        if self.local_loading || self.loading || self.room.is_empty() {
            return;
        }
        self.attention.history(&self.room, &self.rows, &self.you);
        if anchor
            && let Some(first) = self.attention.first_unread(&self.room)
            && let Some(ix) = self.rows.iter().position(|m| m.id >= first)
        {
            self.scroller.update(cx, |s, cx| s.scroll_to_item(ix, cx));
        }
        self.read_visible(window, cx);
    }
    pub(crate) fn enable_local(&mut self, cx: &mut Context<Self>) {
        if self.profile.is_none() || self.local_loading {
            return;
        }
        self.local_enabled = true;
        self.local_error.clear();
        self.flush_local(cx);
        cx.notify();
    }
    pub(crate) fn forget_local(
        &mut self,
        clear: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.profile.clone() else {
            return;
        };
        self.local_enabled = false;
        self.preferences.save_local = false;
        self.local_save = None;
        self.local_epoch += 1;
        let epoch = self.local_epoch;
        self.local_status = "Erasing saved data…".into();
        self.local_error.clear();
        if clear {
            self.preferences = Default::default();
            self.preferences_saved = false;
            self.notifications = false;
            self.apply_preferences(window, cx);
            self.update_idle(window, cx);
            self.drafts.clear();
            self.draft_mentions.clear();
            self.local_touched.clear();
            self.attention = Attention::default();
            self.reply = None;
            self.files.clear();
            self.staged_images.clear();
            self.clipboard_serial += 1;
            self.clipboard_task = None;
            self.composer
                .update(cx, |s, cx| s.set_value("", window, cx));
        }
        let operation = if clear {
            Operation::Forget(profile)
        } else {
            self.preferences_saved = true;
            Operation::Save(profile, self.local_snapshot(cx))
        };
        match self.local_worker.submit(operation) {
            Ok(rx) => {
                cx.spawn(async move |this, cx| {
                    let result = rx.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        if epoch != this.local_epoch {
                            return;
                        }
                        match result {
                            Ok(Ok(_)) => {
                                this.local_status = "Local storage off; saved copy erased".into()
                            }
                            Ok(Err(e)) => this.local_error = e,
                            Err(_) => this.local_error = "Could not erase local data".into(),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
            Err(e) => self.local_error = e.to_string(),
        }
        cx.notify();
    }
    pub(crate) fn privacy_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        if self
            .privacy_form
            .as_ref()
            .and_then(|f| f.upgrade())
            .is_some()
        {
            return;
        }
        let owner = cx.entity();
        let form = cx.new(|cx| PrivacyForm::new(owner, cx));
        self.privacy_form = Some(form.downgrade());
        window.open_dialog(cx, move |d, _, _| {
            d.title("Preferences and local data")
                .child(form.clone())
                .footer(
                    div().flex().justify_end().child(
                        crate::design::button("close-local")
                            .label("Done")
                            .on_click(|_, w, cx| w.close_dialog(cx)),
                    ),
                )
        });
    }
}
pub struct PrivacyForm {
    owner: WeakEntity<Conversation>,
    confirm: bool,
    details: bool,
    _watch: Subscription,
}
impl PrivacyForm {
    fn new(owner: Entity<Conversation>, cx: &mut Context<Self>) -> Self {
        let watch = cx.observe(&owner, |_, _, cx| cx.notify());
        Self {
            owner: owner.downgrade(),
            confirm: false,
            details: false,
            _watch: watch,
        }
    }
}
impl Render for PrivacyForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let v = owner.read(cx);
        let enabled = v.local_enabled;
        let unavailable = v.profile.is_none() || v.local_loading;
        let busy = v.pending || v.transfer_busy || v.mutation_pending.is_some();
        let status = if !v.local_error.is_empty() {
            v.local_error.clone()
        } else if v.local_status.is_empty() {
            "Off by default".into()
        } else {
            v.local_status.clone()
        };
        let toggle = owner.clone();
        let reset = owner.clone();
        let theme = owner.clone();
        let notify = owner.clone();
        let typing = owner.clone();
        let idle = owner.clone();
        let notifications = v.notifications;
        let suppress_typing = v.preferences.suppress_typing;
        let report_idle = v.preferences.report_idle;
        div().id("preferences-scroll").max_h(crate::design::dialog_body_height(window)).overflow_y_scroll().flex().flex_col().gap_3()
   .child(div().text_sm().child("Appearance and privacy choices are saved for this server and user."))
   .child(div().flex().flex_wrap().gap_2()
    .child(crate::design::button("pref-theme").label("Switch light / dark").disabled(unavailable).on_click(move|_,w,cx|theme.update(cx,|v,cx|v.change_preference(Preference::Theme,w,cx))))
    .child(crate::design::button("pref-notify").label(if notifications {"Notifications: on"}else{"Notifications: off"}).disabled(unavailable).on_click(move|_,w,cx|notify.update(cx,|v,cx|v.change_preference(Preference::Notifications,w,cx))))
    .child(crate::design::button("pref-typing").label(if suppress_typing {"Typing: private"}else{"Typing: shared"}).disabled(unavailable).on_click(move|_,w,cx|typing.update(cx,|v,cx|v.change_preference(Preference::Typing,w,cx))))
    .child(crate::design::button("pref-idle").label(if report_idle {"Report idle: on"}else{"Report idle: off"}).disabled(unavailable).on_click(move|_,w,cx|idle.update(cx,|v,cx|v.change_preference(Preference::Idle,w,cx)))))
   .when(self.details,|d|d.child(div().text_sm().child("Save drafts, read cursors and bounded attention IDs/mention flags for this server and user. This profile never stores tokens, message bodies or attachments."))
   .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Draft text is stored unencrypted in a private file under XDG_STATE_HOME/apron-gpui. Up to 256 rooms, 256 KiB of drafts, 512 KiB per profile and 16 profiles. Sign in again as the same user to restore it.")))
   .child(div().text_sm().child(status))
   .when(!self.confirm,|d|d.child(div().flex().flex_wrap().gap_2()
    .child(crate::design::button("local-toggle").label(if enabled{"Disable draft storage"}else{"Save drafts and reads"}).disabled(unavailable).on_click(move|_,w,cx|toggle.update(cx,|v,cx|if enabled{v.forget_local(false,w,cx)}else{v.enable_local(cx)})))
    .child(crate::design::button("local-reset").label("Reset profile…").disabled(unavailable||busy).on_click(cx.listener(|this,_,_,cx|{this.confirm=true;cx.notify();})))) )
   .child(crate::design::button("storage-details").ghost().label(if self.details {"Hide storage details"}else{"Storage details…"}).on_click(cx.listener(|v,_,_,cx|{v.details=!v.details;cx.notify();})))
   .when(self.confirm,|d|d.child(div().flex().flex_col().gap_2().child("Reset appearance, notification and privacy choices, erase saved drafts and read cursors, clear this session’s drafts and selected files, and turn storage off? Messages on the server and your sign-in are unchanged.")
    .child(div().flex().gap_2().child(crate::design::button("cancel-reset").label("Cancel").on_click(cx.listener(|this,_,_,cx|{this.confirm=false;cx.notify();})))
    .child(crate::design::button("confirm-reset").label("Erase local data").on_click(cx.listener(move|this,_,w,cx|{reset.update(cx,|v,cx|v.forget_local(true,w,cx));this.confirm=false;cx.notify();}))))))
   .into_any_element()
    }
}
