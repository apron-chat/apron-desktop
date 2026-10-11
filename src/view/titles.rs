use super::*;
pub(crate) struct TitleForm {
    owner: WeakEntity<Conversation>,
    index: usize,
    generation: u64,
    revision: u64,
    pub(crate) input: Entity<InputState>,
    pub(crate) error: String,
}
impl TitleForm {
    pub(crate) fn submit(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let title = match apron_gpui::edit::title(&self.input.read(cx).value()) {
            Ok(v) => v,
            Err(e) => {
                self.error = e.to_string();
                cx.notify();
                return;
            }
        };
        let result = self
            .owner
            .update(cx, |v, cx| {
                if v.generation != self.generation
                    || v.attachment_revision != self.revision
                    || v.pending
                    || v.transfer_busy
                    || v.clipboard_task.is_some()
                {
                    return false;
                }
                if v.files
                    .iter()
                    .enumerate()
                    .any(|(i, f)| i != self.index && f.title == title)
                {
                    return false;
                }
                let Some(file) = v.files.get_mut(self.index) else {
                    return false;
                };
                let old = std::mem::replace(&mut file.title, title.clone());
                if let Some(image) = v.staged_images.remove(&old) {
                    v.staged_images.insert(title, image);
                }
                v.attachment_revision += 1;
                cx.notify();
                true
            })
            .unwrap_or(false);
        if result {
            w.close_dialog(cx);
        } else {
            self.error="The attachments changed, are busy, or already use this title. Close and review them.".into();
            cx.notify();
        }
    }
}
impl Render for TitleForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::design::surface(cx)
            .child("This title is shared when you send. The local source file is not renamed.")
            .child(
                Input::new(&self.input)
                    .aria_label("Shared attachment title")
                    .w_full(),
            )
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
    pub(crate) fn rename_attachment(
        &mut self,
        index: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if w.has_active_dialog(cx)
            || self.pending
            || self.transfer_busy
            || self.clipboard_task.is_some()
        {
            return;
        }
        let Some(file) = self.files.get(index) else {
            return;
        };
        let title = file.title.clone();
        let owner = cx.weak_entity();
        let generation = self.generation;
        let revision = self.attachment_revision;
        let form = cx.new(|cx| {
            let input = cx.new(|cx| {
                let mut s = InputState::new(w, cx);
                s.set_value(title, w, cx);
                s.focus(w, cx);
                s
            });
            TitleForm {
                owner,
                index,
                generation,
                revision,
                input,
                error: String::new(),
            }
        });
        self.title_form = Some(form.downgrade());
        w.open_dialog(cx, move |d, _, _| {
            let save = form.clone();
            d.title("Rename attachment").child(form.clone()).footer(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        crate::design::button("cancel-title")
                            .label("Cancel")
                            .on_click(|_, w, cx| w.close_dialog(cx)),
                    )
                    .child(
                        crate::design::button("save-title")
                            .primary()
                            .label("Use title")
                            .on_click(move |_, w, cx| save.update(cx, |v, cx| v.submit(w, cx))),
                    ),
            )
        });
    }
}
