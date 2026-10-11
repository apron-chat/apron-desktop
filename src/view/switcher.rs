use super::*;
actions!(switcher, [SwitchNext, SwitchPrevious, SwitchAccept]);
pub struct Switcher {
    owner: WeakEntity<Conversation>,
    pub(crate) input: Entity<InputState>,
    rooms: Vec<(String, String)>,
    index: usize,
    _subscription: Subscription,
}
impl Switcher {
    pub(crate) fn results(&self, cx: &App) -> Vec<(String, String)> {
        let q = self.input.read(cx).value();
        let mut rooms: Vec<_> = self
            .rooms
            .iter()
            .filter_map(|(id, title)| {
                apron_gpui::navigation::score(&q, &format!("{title} {id}"))
                    .map(|score| (score, id, title))
            })
            .collect();
        rooms.sort_by(|a, b| (a.0, a.2).cmp(&(b.0, b.2)));
        rooms
            .into_iter()
            .take(5)
            .map(|(_, id, title)| (id.clone(), title.clone()))
            .collect()
    }
    fn accept(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some((id, _)) = self.results(cx).get(self.index).cloned() {
            window_select(&self.owner, id, w, cx);
        }
    }
}
fn window_select(owner: &WeakEntity<Conversation>, id: String, w: &mut Window, cx: &mut App) {
    w.close_dialog(cx);
    let _ = owner.update(cx, |v, cx| {
        v.select(id, w, cx);
        v.composer.update(cx, |s, cx| s.focus(w, cx));
    });
}
impl Render for Switcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let results = self.results(cx);
        self.index = self.index.min(results.len().saturating_sub(1));
        let mut pane = crate::design::surface(cx)
            .key_context("RoomSwitcher")
            .on_action(cx.listener(|v, _: &SwitchNext, _, cx| {
                v.index = (v.index + 1) % v.results(cx).len().max(1);
                cx.notify();
            }))
            .on_action(cx.listener(|v, _: &SwitchPrevious, _, cx| {
                let n = v.results(cx).len().max(1);
                v.index = (v.index + n - 1) % n;
                cx.notify();
            }))
            .on_action(cx.listener(|v, _: &SwitchAccept, w, cx| v.accept(w, cx)))
            .child(
                Input::new(&self.input)
                    .aria_label("Switch room or thread")
                    .w_full(),
            );
        if results.is_empty() {
            pane = pane.child("No matching rooms");
        }
        for (index, (id, title)) in results.into_iter().enumerate() {
            let owner = self.owner.clone();
            pane = pane.child(
                crate::design::button(SharedString::from(format!("switch-{id}")))
                    .ghost()
                    .selected(index == self.index)
                    .when(index == self.index, |b| {
                        b.bg(cx.theme().accent).text_color(cx.theme().primary)
                    })
                    .justify_start()
                    .w_full()
                    .child(div().w_full().truncate().child(title))
                    .on_click(move |_, w, cx| window_select(&owner, id.clone(), w, cx)),
            );
        }
        pane.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("↑ ↓ choose · Enter open · Escape cancel"),
        )
    }
}
impl Conversation {
    pub(crate) fn open_switcher(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if w.has_active_dialog(cx) {
            return;
        }
        let rooms = self
            .rooms
            .iter()
            .map(|(id, room)| {
                let title = model::clean(room["title"].as_str().unwrap_or(id), 80);
                let label = room["parent_room_id"]
                    .as_str()
                    .map(|p| {
                        format!(
                            "{} / {title}",
                            model::clean(
                                self.rooms
                                    .get(p)
                                    .and_then(|r| r["title"].as_str())
                                    .unwrap_or(p),
                                80
                            )
                        )
                    })
                    .unwrap_or(title);
                (id.clone(), label)
            })
            .collect();
        let owner = cx.weak_entity();
        let form = cx.new(|cx| {
            let input =
                cx.new(|cx| InputState::new(w, cx).placeholder("Jump to a room or thread…"));
            input.update(cx, |s, cx| s.focus(w, cx));
            let subscription = cx.observe(&input, |v: &mut Switcher, _, cx| {
                v.index = 0;
                cx.notify();
            });
            Switcher {
                owner: owner.clone(),
                input,
                rooms,
                index: 0,
                _subscription: subscription,
            }
        });
        self.switcher_form = Some(form.downgrade());
        w.open_dialog(cx, move |d, _, _| {
            let owner = owner.clone();
            d.title("Switch conversation")
                .child(form.clone())
                .on_close(move |_, w, cx| {
                    let _ = owner.update(cx, |v, cx| v.composer.update(cx, |s, cx| s.focus(w, cx)));
                })
        });
    }
}
