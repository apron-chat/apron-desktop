use super::*;
pub struct AvatarUpload {
    owner: WeakEntity<Conversation>,
    generation: u64,
    file: Attachment,
    image: Arc<Image>,
    pub(crate) pending: Option<u64>,
    error: String,
    uncertain: bool,
    closed: bool,
}
impl AvatarUpload {
    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        if self.pending.take().is_some() {
            let _=self.owner.update(cx,|v,cx|{v.command(Command::CancelTransfer,cx);v.error="Avatar upload cancelled; completion may be uncertain. Inspect your profile before retrying.".into();});
        }
    }

    pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
        if self.closed || self.pending.is_some() || self.uncertain {
            return;
        }
        let result = self
            .owner
            .update(cx, |v, cx| {
                if self.generation != v.generation || !v.ready || v.transfer_busy {
                    return None;
                }
                v.mutation_serial += 1;
                let op = v.mutation_serial;
                if v.command(
                    Command::UploadAvatar {
                        operation: op,
                        file: self.file.clone(),
                    },
                    cx,
                ) {
                    v.transfer_busy = true;
                    Some(op)
                } else {
                    None
                }
            })
            .ok()
            .flatten();
        self.pending = result;
        if result.is_none() {
            self.error =
                "Connection changed or another transfer is active. Close and retry.".into();
        }
        cx.notify();
    }
    pub(crate) fn finish(
        &mut self,
        operation: u64,
        result: Result<Value, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closed || self.pending != Some(operation) {
            return;
        }
        self.pending = None;
        match result {
            Ok(_) => {
                self.closed = true;
                w.close_dialog(cx)
            }
            Err(e) => {
                self.uncertain = e.contains("uncertain");
                self.error = e;
                cx.notify();
            }
        }
    }
}
impl Render for AvatarUpload {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::design::surface(cx)
        .child("Replace your shared avatar? This image is uploaded to this server and may be visible to other users. It is not sent as a chat message.")
        .child(img(self.image.clone()).size_24().object_fit(ObjectFit::Contain))
        .child("A normalized PNG is uploaded; original metadata is removed.")
        .when(!self.error.is_empty(),|d|d.child(div().text_color(cx.theme().danger).text_sm().child(self.error.clone())))
    }
}
impl Conversation {
    pub(crate) fn avatar_upload_dialog(
        &mut self,
        raster: apron_gpui::images::Raster,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if w.has_active_dialog(cx) {
            return;
        }
        let owner = cx.weak_entity();
        let form = cx.new(|_| AvatarUpload {
            owner,
            generation: self.generation,
            file: Attachment {
                title: "avatar.png".into(),
                bytes: raster.png,
            },
            image: Arc::new(Image::from_bytes(ImageFormat::Png, raster.thumbnail)),
            pending: None,
            error: String::new(),
            uncertain: false,
            closed: false,
        });
        self.avatar_upload = Some(form.downgrade());
        w.open_dialog(cx, move |d, _, cx| {
            let submit = form.clone();
            let close = form.clone();
            let cancel = form.clone();
            let busy = form.read(cx).pending.is_some();
            let disabled = busy || form.read(cx).uncertain;
            d.title("Set my avatar")
                .child(form.clone())
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            crate::design::button("avatar-cancel")
                                .label(if busy { "Cancel upload" } else { "Cancel" })
                                .on_click(move |_, w, cx| {
                                    cancel.update(cx, |f, cx| f.cancel(cx));
                                    w.close_dialog(cx)
                                }),
                        )
                        .child(
                            crate::design::button("avatar-confirm")
                                .primary()
                                .label(if busy {
                                    "Uploading…"
                                } else {
                                    "Confirm upload"
                                })
                                .disabled(disabled)
                                .on_click(move |_, _, cx| submit.update(cx, |f, cx| f.submit(cx))),
                        ),
                )
                .on_close(move |_, _, cx| close.update(cx, |f, cx| f.cancel(cx)))
        });
    }
    pub(crate) fn choose_avatar(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.ready
            || self.transfer_busy
            || self.clipboard_task.is_some()
            || w.has_active_dialog(cx)
        {
            return;
        }
        if !["command", "embed:upload"]
            .iter()
            .all(|c| self.caps.iter().any(|v| v == c))
        {
            self.error = "This server does not support avatar uploads.".into();
            cx.notify();
            return;
        }
        let generation = self.generation;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose an avatar image · at most 8 MiB".into()),
        });
        self.clipboard_serial += 1;
        let serial = self.clipboard_serial;
        self.clipboard_task = Some(cx.spawn_in(w, async move |v, cx| {
            let selected = prompt.await;
            let result = match selected {
                Ok(Ok(Some(paths))) => {
                    cx.background_executor()
                        .spawn(async move {
                            let files = apron_gpui::media::read_local(paths)?;
                            let f = files
                                .first()
                                .ok_or_else(|| anyhow::anyhow!("Choose one image"))?;
                            apron_gpui::images::normalize(&f.bytes).map(Some)
                        })
                        .await
                }
                Ok(Ok(None)) => Ok(None),
                _ => Err(anyhow::anyhow!(
                    "File chooser unavailable; check your desktop portal"
                )),
            };
            let _ = v.update_in(cx, |v, w, cx| {
                if generation != v.generation || serial != v.clipboard_serial {
                    return;
                }
                v.clipboard_task = None;
                match result {
                    Ok(Some(r)) => v.avatar_upload_dialog(r, w, cx),
                    Ok(None) => {}
                    Err(e) => {
                        v.error = format!(
                            "Could not prepare avatar: {}",
                            model::clean(&e.to_string(), 240)
                        )
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
