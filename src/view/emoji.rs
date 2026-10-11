use super::*;
pub(crate) struct EmojiForm {
    owner: WeakEntity<Conversation>,
    message: u64,
    generation: u64,
    pub(crate) input: Entity<InputState>,
    results: Vec<&'static emojis::Emoji>,
    page: usize,
    selected: Option<&'static emojis::Emoji>,
    pub(crate) error: String,
    busy: bool,
    uncertain: bool,
    _watch: Vec<Subscription>,
}
impl EmojiForm {
    pub(crate) fn choose(&mut self, value: &str, cx: &mut Context<Self>) {
        self.selected = emojis::get(value).filter(|e| e.as_str().len() <= 64);
        self.error.clear();
        cx.notify();
    }

    pub(crate) fn finished(
        &mut self,
        result: Result<(), String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.busy {
            return;
        }
        self.busy = false;
        match result {
            Ok(()) => w.close_dialog(cx),
            Err(e) => {
                self.uncertain = e.contains("uncertain");
                self.error = e;
                cx.notify();
            }
        }
    }
    pub(crate) fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.uncertain {
            return;
        }
        let Some(emoji) = self.selected else {
            self.error = "Choose an emoji first.".into();
            cx.notify();
            return;
        };
        let generation = self.generation;
        let id = self.message;
        self.busy = self
            .owner
            .update(cx, |v, cx| {
                if generation != v.generation || !v.can_react(id) {
                    return false;
                }
                v.error.clear();
                v.react(id, emoji.as_str(), cx);
                v.reacting == Some(id)
            })
            .unwrap_or(false);
        if !self.busy {
            self.error = "This message is no longer available for reactions.".into();
        }
        cx.notify();
    }
}
impl Render for EmojiForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.results.len().div_ceil(18).max(1);
        self.page = self.page.min(total - 1);
        let mut grid = div().flex().flex_wrap().gap_1();
        for e in self.results.iter().skip(self.page * 18).take(18).copied() {
            grid = grid.child(
                crate::design::button(SharedString::from(format!("emoji-{}", e.as_str())))
                    .w_12()
                    .label(e.as_str())
                    .accessibility_label(e.name())
                    .tooltip(e.name())
                    .selected(self.selected == Some(e))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.choose(e.as_str(), cx);
                    })),
            );
        }
        crate::design::surface(cx)
            .child(
                Input::new(&self.input)
                    .aria_label("Search emoji names or paste an emoji")
                    .disabled(self.busy),
            )
            .child(grid)
            .when(self.results.is_empty(), |d| {
                d.child("No emoji matches. Try a name or paste a Unicode emoji.")
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        crate::design::button("emoji-prev")
                            .label("Previous")
                            .disabled(self.page == 0 || self.busy)
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.page = v.page.saturating_sub(1);
                                cx.notify();
                            })),
                    )
                    .child(format!("{} / {total}", self.page + 1))
                    .child(
                        crate::design::button("emoji-next")
                            .label("Next")
                            .disabled(self.page + 1 >= total || self.busy)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.page += 1;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div().text_sm().child(
                    self.selected
                        .map(|e| format!("Selected: {} {}", e.as_str(), e.name()))
                        .unwrap_or_else(|| {
                            "Choose an emoji, then toggle your reaction. Tab moves between choices."
                                .into()
                        }),
                ),
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
    pub(crate) fn emoji_dialog(&mut self, id: u64, w: &mut Window, cx: &mut Context<Self>) {
        if w.has_active_dialog(cx) || !self.can_react(id) {
            return;
        }
        let owner = cx.entity();
        let generation = self.generation;
        let form = cx.new(|cx| {
            let input = cx.new(|cx| InputState::new(w, cx).placeholder("Search or paste emoji…"));
            input.update(cx, |s, cx| s.focus(w, cx));
            let watch = cx.observe(&input, |v: &mut EmojiForm, _, cx| {
                v.results = apron_gpui::emoji::search(&v.input.read(cx).value());
                v.page = 0;
                cx.notify();
            });
            let state = cx.observe(&owner, |_, _, cx| cx.notify());
            EmojiForm {
                owner: owner.downgrade(),
                message: id,
                generation,
                input,
                results: apron_gpui::emoji::search(""),
                page: 0,
                selected: None,
                error: String::new(),
                busy: false,
                uncertain: false,
                _watch: vec![watch, state],
            }
        });
        self.emoji_form = Some(form.downgrade());
        w.open_dialog(cx, move |d, _, cx| {
            let submit = form.clone();
            let disabled =
                form.read(cx).busy || form.read(cx).uncertain || form.read(cx).selected.is_none();
            d.title("Choose a reaction").child(form.clone()).footer(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        crate::design::button("emoji-cancel")
                            .label("Close")
                            .on_click(|_, w, cx| w.close_dialog(cx)),
                    )
                    .child(
                        crate::design::button("emoji-submit")
                            .primary()
                            .label("Toggle reaction")
                            .disabled(disabled)
                            .on_click(move |_, w, cx| submit.update(cx, |f, cx| f.submit(w, cx))),
                    ),
            )
        });
    }
}
