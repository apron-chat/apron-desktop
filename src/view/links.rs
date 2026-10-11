use super::*;
impl Conversation {
    pub(crate) fn links_dialog(&self, id: u64, w: &mut Window, cx: &mut Context<Self>) {
        if w.has_active_dialog(cx) {
            return;
        }
        let urls = self
            .store
            .get(id)
            .map(|m| apron_gpui::links::candidates(model::text(&m.raw["body"]["text"])))
            .unwrap_or_default();
        w.open_dialog(cx, move |d, _, cx| {
            let mut pane = crate::design::surface(cx).child(
                "Choose a link to open in your browser. The site may learn your IP address.",
            );
            if urls.is_empty() {
                pane = pane.child("No supported HTTP or HTTPS links in this message.");
            }
            for (index, url) in urls.iter().enumerate() {
                let target = url.clone();
                pane = pane.child(
                    crate::design::button(("external-link", index))
                        .w_full()
                        .child(div().truncate().child(model::clean(url, 200)))
                        .tooltip(url.clone())
                        .on_click(move |_, _, cx| cx.open_url(&target)),
                );
            }
            d.title("External links").child(pane)
        });
    }
}
