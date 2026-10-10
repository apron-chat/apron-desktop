use super::*;
impl Conversation {
    pub(crate) fn match_ids(&self, _cx: &App) -> &[u64] {
        &self.search_matches
    }
    pub(crate) fn update_matches(&mut self, cx: &App) {
        let query = self.search.read(cx).value().trim().to_lowercase();
        self.search_query = query.clone();
        self.search_matches = if query.is_empty() {
            Vec::new()
        } else {
            self.rows
                .iter()
                .filter(|m| {
                    model::text(&m.raw["body"]["text"])
                        .to_lowercase()
                        .contains(&query)
                })
                .map(|m| m.id)
                .collect()
        };
        self.search_index = self
            .search_index
            .min(self.search_matches.len().saturating_sub(1));
    }
    pub(crate) fn open_search(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.searching = true;
        self.search.update(cx, |s, cx| s.focus(w, cx));
        cx.notify();
    }
    pub(crate) fn reveal_match(&mut self, cx: &mut Context<Self>) {
        let matches = self.search_matches.clone();
        self.search_index = self.search_index.min(matches.len().saturating_sub(1));
        if let Some(id) = matches.get(self.search_index)
            && let Some(ix) = self.rows.iter().position(|m| m.id == *id)
        {
            self.scroller.update(cx, |s, cx| s.scroll_to_item(ix, cx));
        }
    }
    pub(crate) fn step_match(&mut self, next: bool, cx: &mut Context<Self>) {
        if !self.searching {
            return;
        }
        let n = self.match_ids(cx).len();
        if n == 0 {
            return;
        }
        self.search_index = if next {
            (self.search_index + 1) % n
        } else {
            (self.search_index + n - 1) % n
        };
        self.reveal_match(cx);
        cx.notify();
    }
    pub(crate) fn search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let n = self.match_ids(cx).len();
        div()
            .px_4()
            .py_2()
            .bg(cx.theme().muted)
            .flex()
            .items_center()
            .gap_2()
            .child(
                Input::new(&self.search)
                    .prefix(IconName::Search)
                    .aria_label("Search loaded messages")
                    .flex_1()
                    .min_w_0(),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if n == 0 {
                        "No matches · loaded only".into()
                    } else {
                        format!("{} / {n} · loaded only", self.search_index.min(n - 1) + 1)
                    }),
            )
            .child(
                Button::new("previous-match")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronUp)
                    .accessibility_label("Previous match")
                    .tooltip("Previous match · Shift+F3")
                    .disabled(n == 0)
                    .on_click(cx.listener(|v, _, _, cx| v.step_match(false, cx))),
            )
            .child(
                Button::new("next-match")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronDown)
                    .accessibility_label("Next match")
                    .tooltip("Next match · F3")
                    .disabled(n == 0)
                    .on_click(cx.listener(|v, _, _, cx| v.step_match(true, cx))),
            )
            .child(
                Button::new("close-search")
                    .ghost()
                    .small()
                    .icon(IconName::Close)
                    .accessibility_label("Close search")
                    .on_click(cx.listener(|v, _, w, cx| {
                        v.searching = false;
                        v.composer.update(cx, |s, cx| s.focus(w, cx));
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    pub(crate) fn can_react(&self, id: u64) -> bool {
        self.ready
            && self.reacting.is_none()
            && self.joined()
            && self.caps.iter().any(|c| c == "reactions")
            && self
                .store
                .get(id)
                .is_some_and(|m| m.room == self.room && !m.deleted())
    }
    pub(crate) fn react(&mut self, id: u64, emoji: &str, cx: &mut Context<Self>) {
        if !self.can_react(id) {
            return;
        }
        let mut emojis = self.reactions.own(id, &self.you);
        if !emojis.remove(emoji) {
            if emojis.len() >= 16 {
                self.error = "Remove a reaction before adding another.".into();
                cx.notify();
                return;
            }
            emojis.insert(emoji.into());
        }
        if self.command(
            Command::React {
                generation: self.generation,
                message: id,
                emojis: emojis.into_iter().collect(),
            },
            cx,
        ) {
            self.reacting = Some(id);
        }
        cx.notify();
    }
    pub(crate) fn reaction_chips(&self, id: u64, owner: WeakEntity<Self>, cx: &App) -> AnyElement {
        let own = self.reactions.own(id, &self.you);
        let enabled = self.can_react(id);
        div()
            .flex()
            .flex_wrap()
            .gap_1()
            .children(self.reactions.totals(id).into_iter().enumerate().map(
                |(i, (emoji, count))| {
                    let selected = own.contains(&emoji);
                    let owner = owner.clone();
                    Button::new(("reaction", i))
                        .outline()
                        .xsmall()
                        .rounded_full()
                        .selected(selected)
                        .when(selected, |b| {
                            b.bg(cx.theme().accent).border_color(cx.theme().primary)
                        })
                        .label(format!("{emoji} {count}"))
                        .accessibility_label(format!(
                            "{} reaction, {count}, {}",
                            emoji,
                            if selected { "yours, remove" } else { "add" }
                        ))
                        .disabled(!enabled)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |v, cx| v.react(id, &emoji, cx));
                        })
                },
            ))
            .into_any_element()
    }
}
