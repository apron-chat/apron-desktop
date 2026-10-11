use super::*;
use apron_gpui::mentions::{self, Member};
impl Conversation {
    pub(crate) fn mention_candidates(&self, cx: &App) -> Vec<Member> {
        let state = self.composer.read(cx);
        let Some((_, query)) = mentions::query(&state.value(), state.cursor()) else {
            return vec![];
        };
        let Some(room) = self.rooms.get(&self.room) else {
            return vec![];
        };
        mentions::candidates(&mentions::members(room, &self.users), &query)
    }
    pub(crate) fn add_mention(&mut self, id: &str, w: &mut Window, cx: &mut Context<Self>) {
        let Some(room) = self.rooms.get(&self.room) else {
            return;
        };
        if !mentions::members(room, &self.users)
            .iter()
            .any(|m| m.id == id)
        {
            self.error = "This person is no longer in the supplied member list.".into();
            cx.notify();
            return;
        }
        let current_text = self.composer.read(cx).value();
        let selected = self.draft_mentions.entry(self.room.clone()).or_default();
        selected.retain(|id| mentions::present(&current_text, id));
        if selected.len() >= 32 && !selected.contains(id) {
            self.error = "A message can notify at most 32 selected people.".into();
            cx.notify();
            return;
        }
        let state = self.composer.read(cx);
        let range = mentions::query(&state.value(), state.cursor())
            .map(|(r, _)| r)
            .unwrap_or_else(|| state.selected_range());
        selected.insert(id.into());
        self.composer.update(cx, |s, cx| {
            s.set_selected_range(range, cx);
            s.replace(format!("@{id} "), w, cx);
            s.focus(w, cx);
        });
        self.mention_index = 0;
        self.local_touched.insert(self.room.clone());
        self.schedule_local(cx);
        cx.notify();
    }
    pub(crate) fn mention_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let candidates = self.mention_candidates(cx);
        let query = mentions::query(
            &self.composer.read(cx).value(),
            self.composer.read(cx).cursor(),
        )
        .is_some();
        let mut pane = div().flex().flex_col().gap_1();
        if query {
            pane = pane.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Choose a room member to notify · ↑↓ and Enter"),
            );
            if candidates.is_empty() {
                pane = pane.child(
                    div()
                        .text_sm()
                        .child("No matching members supplied by this server"),
                );
            }
            let mut list = div()
                .id("mention-options")
                .max_h_32()
                .overflow_y_scroll()
                .flex()
                .flex_col();
            for (ix, member) in candidates.iter().enumerate() {
                let id = member.id.clone();
                list = list.child(
                    crate::design::button(SharedString::from(format!("mention-{}", member.id)))
                        .ghost()
                        .small()
                        .justify_start()
                        .selected(ix == self.mention_index % candidates.len())
                        .label(format!("{} · @{}", member.name, member.id))
                        .on_click(cx.listener(move |v, _, w, cx| v.add_mention(&id, w, cx))),
                );
            }
            pane = pane.child(list);
        }
        let text = self.composer.read(cx).value();
        if let Some(selected) = self.draft_mentions.get(&self.room) {
            let ids = mentions::recipients(&text, selected);
            if !ids.is_empty() {
                let mut chips = div()
                    .id("mention-recipients")
                    .max_h_20()
                    .overflow_y_scroll()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Notifies"),
                    );
                for id in ids {
                    let remove = id.clone();
                    chips = chips.child(
                        crate::design::button(SharedString::from(format!("remove-mention-{id}")))
                            .ghost()
                            .xsmall()
                            .label(format!("@{id} ×"))
                            .accessibility_label(format!("Do not notify {id}"))
                            .on_click(cx.listener(move |v, _, _, cx| {
                                if let Some(ids) = v.draft_mentions.get_mut(&v.room) {
                                    ids.remove(&remove);
                                }
                                v.schedule_local(cx);
                                cx.notify();
                            })),
                    );
                }
                pane = pane.child(chips);
            }
        }
        pane.into_any_element()
    }
    pub(crate) fn preview_markdown(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if w.has_active_dialog(cx) {
            return;
        }
        let display =
            SharedString::from(model::display_text(&self.composer.read(cx).value(), true));
        w.open_dialog(cx, move |d, _, _| {
            d.title("Message preview").child(
                TextView::markdown("compose-preview", display.clone())
                    .selectable(true)
                    .on_link_click(|_, _, _, _| {}),
            )
        });
    }
}
