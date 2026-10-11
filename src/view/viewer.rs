use super::*;
pub struct Viewer {
    owner: WeakEntity<Conversation>,
    generation: u64,
    pub(crate) key: String,
    source: String,
    avatar: Option<String>,
    target: Option<String>,
    pub(crate) raster: Option<apron_gpui::images::Raster>,
    image: Option<Arc<Image>>,
    pub(crate) zoom: Option<f32>,
    pub(crate) loading: bool,
    closed: bool,
    error: String,
    saving: bool,
    _watch: Subscription,
}
impl Viewer {
    pub(crate) fn current(&self, v: &Conversation) -> bool {
        !self.closed
            && self.generation == v.generation
            && self.avatar.as_ref().map_or_else(
                || {
                    self.target
                        .as_ref()
                        .is_some_and(|key| v.image_key_current(key))
                },
                |user| v.avatar_source(user).as_deref() == Some(&self.source),
            )
    }
    pub(crate) fn load(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.raster.is_some() {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let result = owner.update(cx, |v, cx| {
            if !self.current(v) || v.transfer_busy {
                return false;
            }
            if let Err(e) =
                apron_gpui::visual_media::allowed(&self.source, &v.server, self.avatar.is_some())
            {
                self.error = e.to_string();
                return false;
            }
            v.transfer_busy = true;
            if v.command(
                Command::LoadImage {
                    avatar: self.avatar.is_some(),
                    generation: self.generation,
                    key: self.key.clone(),
                    url: self.source.clone(),
                },
                cx,
            ) {
                true
            } else {
                v.transfer_busy = false;
                false
            }
        });
        self.loading = result;
        if !result && self.error.is_empty() {
            self.error = "Source changed or another transfer is active. Close and retry.".into();
        }
        cx.notify();
    }
    pub(crate) fn finish(
        &mut self,
        generation: u64,
        key: &str,
        result: Result<apron_gpui::images::Raster, String>,
        current: bool,
        cx: &mut Context<Self>,
    ) {
        if self.closed || generation != self.generation || key != self.key {
            return;
        }
        self.loading = false;
        if !current {
            self.error = "The image source changed. Close and reopen it.".into();
            cx.notify();
            return;
        }
        match result {
            Ok(raster) => {
                self.image = Some(Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    raster.png.clone(),
                )));
                self.raster = Some(raster);
            }
            Err(e) => self.error = model::clean(&e, 240),
        }
        cx.notify();
    }
    pub(crate) fn use_avatar(&mut self, cx: &mut Context<Self>) {
        let Some(user) = self.avatar.clone() else {
            return;
        };
        let Some(raster) = &self.raster else {
            return;
        };
        let image = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            raster.thumbnail.clone(),
        ));
        let _ = self.owner.update(cx, |v, cx| {
            if !self.current(v) {
                return;
            }
            if v.avatar_cache.len() >= 32 {
                v.avatar_cache.pop_first();
            }
            v.avatar_cache.insert(user, (self.source.clone(), image));
            cx.notify();
        });
    }
    pub(crate) fn dismiss(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.close(cx);
        w.close_dialog(cx);
    }
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        self.closed = true;
        if self.loading {
            let _ = self.owner.update(cx, |v, cx| {
                v.command(Command::CancelTransfer, cx);
            });
        }
        self.loading = false;
        self.image = None;
        self.raster = None;
    }
    pub(crate) fn zoom_by(&mut self, factor: f32, cx: &mut Context<Self>) {
        self.zoom = Some(apron_gpui::visual_media::scale(
            self.zoom.unwrap_or(1.),
            factor,
        ));
        cx.notify();
    }
    fn save(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(raster) = &self.raster else {
            return;
        };
        let bytes = raster.png.clone();
        let path = cx.prompt_for_new_path(std::path::Path::new("."), Some("apron-image.png"));
        self.saving = true;
        cx.spawn_in(w, async move |this, cx| {
            let result = path.await;
            let allowed = this
                .update(cx, |v, cx| {
                    !v.closed && v.owner.upgrade().is_some_and(|o| v.current(o.read(cx)))
                })
                .unwrap_or(false);
            if !allowed {
                return;
            }
            let result = match result {
                Ok(Ok(Some(path))) => cx
                    .background_executor()
                    .spawn(async move {
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()?
                            .block_on(apron_gpui::media::save(&path, &bytes))
                    })
                    .await
                    .map(|_| "Normalized PNG saved".to_string())
                    .map_err(|e: anyhow::Error| e.to_string()),
                Ok(Ok(None)) => Ok(String::new()),
                _ => Err("Save dialog unavailable; check your desktop portal.".into()),
            };
            let _ = this.update(cx, |v, cx| {
                v.saving = false;
                v.error = result.unwrap_or_else(|e| e);
                cx.notify();
            });
        })
        .detach();
    }
    fn controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .owner
            .upgrade()
            .is_some_and(|v| self.current(v.read(cx)));
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap_2()
            .when(self.raster.is_some(), |d| {
                d.child(
                    crate::design::button("image-fit")
                        .label("Fit")
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.zoom = None;
                            cx.notify();
                        })),
                )
                .child(
                    crate::design::button("image-one")
                        .label("100%")
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.zoom = Some(1.);
                            cx.notify();
                        })),
                )
                .child(
                    crate::design::button("image-minus")
                        .label("−")
                        .accessibility_label("Zoom out")
                        .on_click(cx.listener(|v, _, _, cx| v.zoom_by(0.5, cx))),
                )
                .child(
                    crate::design::button("image-plus")
                        .label("+")
                        .accessibility_label("Zoom in")
                        .on_click(cx.listener(|v, _, _, cx| v.zoom_by(2., cx))),
                )
                .child(
                    crate::design::button("image-save")
                        .label("Save PNG…")
                        .disabled(self.saving || !current)
                        .on_click(cx.listener(|v, _, w, cx| v.save(w, cx))),
                )
            })
            .when(self.raster.is_some() && self.avatar.is_some(), |d| {
                d.child(
                    crate::design::button("avatar-use")
                        .label("Show in chat")
                        .disabled(!current)
                        .on_click(cx.listener(|v, _, w, cx| {
                            v.use_avatar(cx);
                            v.dismiss(w, cx);
                        })),
                )
            })
            .when(self.raster.is_none(), |d| {
                d.child(
                    crate::design::button("image-load")
                        .primary()
                        .label(if self.loading {
                            "Loading…"
                        } else {
                            "Load image"
                        })
                        .disabled(self.loading || !current)
                        .on_click(cx.listener(|v, _, _, cx| v.load(cx))),
                )
            })
            .child(
                crate::design::button("image-close")
                    .label(if self.loading { "Cancel" } else { "Close" })
                    .on_click(cx.listener(|v, _, w, cx| v.dismiss(w, cx))),
            )
            .into_any_element()
    }
}
impl Render for Viewer {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self
            .owner
            .upgrade()
            .is_some_and(|v| self.current(v.read(cx)));
        let height = (w.viewport_size().height
            - px(if self.avatar.is_some() { 300. } else { 270. }))
        .max(px(100.))
        .min(px(520.));
        let width = (w.viewport_size().width - px(128.))
            .max(px(200.))
            .min(px(836.));
        crate::design::surface(cx)
            .when(!current, |d| {
                d.child("The source changed. Close and reopen to review the current image.")
            })
            .child(
                div()
                    .text_sm()
                    .child("Explicit image loading · Save writes a metadata-free PNG."),
            )
            .when_some(self.raster.as_ref(), |d, r| {
                d.child(format!(
                    "{} × {} · {}",
                    r.width,
                    r.height,
                    self.zoom
                        .map(|s| format!("{:.0}%", s * 100.))
                        .unwrap_or("Fit".into())
                ))
            })
            .when_some(self.image.clone(), |d, image| {
                let r = self.raster.as_ref().unwrap();
                let scale = self.zoom.unwrap_or(
                    (f32::from(width) / r.width as f32)
                        .min(f32::from(height) / r.height as f32)
                        .min(1.),
                );
                d.child(
                    div()
                        .id("raster-scroll")
                        .h(height)
                        .w_full()
                        .overflow_scroll()
                        .child(
                            img(image)
                                .w(px(r.width as f32 * scale))
                                .h(px(r.height as f32 * scale))
                                .object_fit(ObjectFit::Contain),
                        ),
                )
            })
            .when(!self.error.is_empty(), |d| {
                d.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(self.error.clone()),
                )
            })
    }
}
impl Conversation {
    pub(crate) fn avatar_source(&self, user: &str) -> Option<String> {
        self.users
            .get(user)
            .and_then(|u| u["avatar"].as_str())
            .filter(|s| !s.is_empty() && s.len() <= apron_gpui::visual_media::AVATAR_SOURCE_LIMIT)
            .map(str::to_owned)
    }
    pub(crate) fn avatar_image(
        user: &str,
        owner: &WeakEntity<Self>,
        cx: &App,
    ) -> Option<Arc<Image>> {
        let owner = owner.upgrade()?;
        let v = owner.read(cx);
        let (source, image) = v.avatar_cache.get(user)?;
        (v.avatar_source(user).as_ref() == Some(source)).then(|| image.clone())
    }
    pub(crate) fn avatar_dialog(&mut self, user: String, w: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.avatar_source(&user) else {
            self.error = "This user has no supported avatar. Initials are shown.".into();
            cx.notify();
            return;
        };
        self.open_viewer(source, None, Some(user), w, cx);
    }
    pub(crate) fn open_viewer(
        &mut self,
        source: String,
        target: Option<String>,
        avatar: Option<String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if w.has_active_dialog(cx) {
            return;
        }
        self.mutation_serial += 1;
        let key = format!("viewer:{}", self.mutation_serial);
        let owner = cx.entity();
        let error = apron_gpui::visual_media::allowed(&source, &self.server, avatar.is_some())
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        let form = cx.new(|cx| Viewer {
            owner: owner.downgrade(),
            generation: self.generation,
            key,
            source,
            avatar,
            target,
            raster: None,
            image: None,
            zoom: None,
            loading: false,
            closed: false,
            error,
            saving: false,
            _watch: cx.observe(&owner, |_, _, cx| cx.notify()),
        });
        self.viewer = Some(form.downgrade());
        w.open_dialog(cx, move |d, w, cx| {
            let close = form.clone();
            d.width((w.viewport_size().width - px(64.)).min(px(900.)))
                .title("Image viewer")
                .child(form.clone())
                .footer(form.update(cx, |f, cx| f.controls(cx)))
                .on_close(move |_, _, cx| close.update(cx, |f, cx| f.close(cx)))
        });
    }
}
