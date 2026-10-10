use super::*;
/// Scope lifetime follows the rendered virtual row; no global per-message focus map.
pub(super) struct RowFocus {
    pub handle: FocusHandle,
    pub bounds: Bounds<Pixels>,
    _subscription: Subscription,
}
impl RowFocus {
    pub fn new(
        id: u64,
        owner: WeakEntity<Conversation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let handle = cx.focus_handle();
        let subscription = cx.on_focus_in(&handle, window, move |_, w, cx| {
            if !w.last_input_was_keyboard() {
                return;
            }
            let state = cx.weak_entity();
            let owner = owner.clone();
            // Focus listeners run during frame completion. Scroll after that frame,
            // so the list's completed layout cannot overwrite the requested offset.
            w.on_next_frame(move |w, cx| {
                let _ = state.update(cx, |this, cx| {
                    if !w.last_input_was_keyboard() || !this.handle.contains_focused(w, cx) {
                        return;
                    }
                    let _ = owner.update(cx, |v, cx| {
                        let viewport = *v.transcript_bounds.borrow();
                        let header_bottom = this.bounds.top() + w.rem_size() * 2.;
                        if (this.bounds.top() < viewport.top() + px(4.)
                            || header_bottom > viewport.bottom() - px(4.))
                            && let Some(ix) = v.rows.iter().position(|m| m.id == id)
                        {
                            v.scroller.update(cx, |s, cx| s.scroll_to_item(ix, cx));
                        }
                    });
                });
            });
        });
        Self {
            handle,
            bounds: Bounds::default(),
            _subscription: subscription,
        }
    }
}
