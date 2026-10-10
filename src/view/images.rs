use super::*;
impl Conversation {
    pub(crate) fn paste_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let image = cx.read_from_clipboard().and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.bytes.clone()),
                _ => None,
            })
        });
        let Some(bytes) = image else {
            window.dispatch_action(Box::new(gpui_kit::component::input::Paste), cx);
            return;
        };
        if !self.caps.iter().any(|c| c == "embed:upload") {
            self.error =
                "This server does not support image uploads. Your message is unchanged.".into();
            cx.notify();
            return;
        }
        if self.pending
            || self.transfer_busy
            || self.files.len() >= 4
            || self.clipboard_task.is_some()
        {
            self.error = "Finish the current operation or remove an attachment before pasting another image.".into();
            cx.notify();
            return;
        }
        if bytes.len() > apron_gpui::media::LIMIT {
            self.error = "Clipboard image exceeds 8 MiB.".into();
            cx.notify();
            return;
        }
        let generation = self.generation;
        self.clipboard_serial += 1;
        let serial = self.clipboard_serial;
        self.transfer = "Preparing clipboard image…".into();
        let decode = cx
            .background_executor()
            .spawn(async move { apron_gpui::images::normalize(&bytes) });
        self.clipboard_task = Some(cx.spawn(async move |this, cx| {
            let result = decode.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation || serial != this.clipboard_serial {
                    return;
                }
                this.clipboard_task = None;
                match result {
                    Ok(raster)
                        if this.files.len() < 4
                            && this.files.iter().map(|f| f.bytes.len()).sum::<usize>()
                                + raster.png.len()
                                <= apron_gpui::media::LIMIT =>
                    {
                        let title = format!("Clipboard image {serial}.png");
                        this.staged_images.insert(
                            title.clone(),
                            Arc::new(Image::from_bytes(ImageFormat::Png, raster.thumbnail)),
                        );
                        this.files.push(Attachment {
                            title,
                            bytes: raster.png,
                        });
                        this.transfer = format!(
                            "Image ready · {} × {} · review before sending",
                            raster.width, raster.height
                        );
                        this.error.clear();
                    }
                    Ok(_) => {
                        this.error = "Attachments must total at most 8 MiB and four files.".into()
                    }
                    Err(e) => {
                        this.error = format!(
                            "Could not paste image: {}",
                            model::clean(&e.to_string(), 240)
                        )
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(crate) fn staged_images_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut d = div()
            .id("staged-attachments")
            .max_h_32()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        if self.clipboard_task.is_some() {
            d = d.child(
                Button::new("cancel-paste")
                    .small()
                    .label("Cancel image preparation")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.clipboard_serial += 1;
                        v.clipboard_task = None;
                        v.transfer.clear();
                        cx.notify();
                    })),
            );
        }
        for (ix, file) in self.files.iter().enumerate() {
            let name = file.title.clone();
            let mut item = div().flex().items_center().gap_2();
            if let Some(preview) = self.staged_images.get(&name) {
                item = item.child(
                    img(preview.clone())
                        .w_20()
                        .h_16()
                        .object_fit(ObjectFit::Contain),
                );
            }
            item = item
                .child(div().flex_1().min_w_0().truncate().text_sm().child(format!(
                    "{} · {} KiB",
                    name,
                    file.bytes.len().div_ceil(1024)
                )))
                .child(
                    Button::new(("remove-file", ix))
                        .ghost()
                        .small()
                        .label("Remove")
                        .disabled(self.pending)
                        .on_click(cx.listener(move |v, _, _, cx| {
                            if let Some(pos) = v.files.iter().position(|f| f.title == name) {
                                v.files.remove(pos);
                            }
                            v.staged_images.remove(&name);
                            cx.notify();
                        })),
                );
            d = d.child(item);
        }
        d.into_any_element()
    }
    pub(crate) fn image_key(id: u64, ix: usize, url: &str) -> String {
        format!("{id}:{ix}:{url}")
    }
    pub(crate) fn image_key_current(&self, key: &str) -> bool {
        self.rows
            .iter()
            .filter(|m| !m.raw["deleted"].as_bool().unwrap_or(false))
            .any(|m| {
                m.raw["body"]["embeds"].as_array().is_some_and(|es| {
                    es.iter().take(4).enumerate().any(|(ix, e)| {
                        e["url"]
                            .as_str()
                            .is_some_and(|url| Self::image_key(m.id, ix, url) == key)
                    })
                })
            })
    }
    pub(crate) fn embedded_image(
        id: u64,
        ix: usize,
        url: String,
        owner: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let Some(entity) = owner.upgrade() else {
            return div().into_any_element();
        };
        let v = entity.read(cx);
        let key = Self::image_key(id, ix, &url);
        if let Some(image) = v.image_cache.get(&key) {
            return div()
                .max_w_full()
                .child(
                    img(image.clone())
                        .max_w_full()
                        .w_96()
                        .h_48()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element();
        }
        let allowed = apron_gpui::media::transfer_url(&url, &v.server).is_ok();
        let disabled = !v.ready || v.transfer_busy || !allowed;
        let error = v.image_errors.get(&key).cloned();
        let owner = owner.clone();
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("image-{id}-{ix}")))
                    .ghost()
                    .small()
                    .label("Load image from this server")
                    .disabled(disabled)
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |v, cx| {
                            if !v.image_key_current(&key) || v.transfer_busy {
                                return;
                            }
                            v.image_errors.remove(&key);
                            v.transfer_busy = true;
                            if !v.command(
                                Command::LoadImage {
                                    generation: v.generation,
                                    key: key.clone(),
                                    url: url.clone(),
                                },
                                cx,
                            ) {
                                v.transfer_busy = false;
                            }
                        });
                    }),
            )
            .when_some(error, |d, e| {
                d.child(div().text_xs().text_color(cx.theme().danger).child(e))
            })
            .when(!allowed, |d| {
                d.child(div().text_xs().child("External image loading is blocked."))
            })
            .into_any_element()
    }
}
