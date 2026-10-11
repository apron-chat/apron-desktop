use super::*;
use apron_gpui::edit::payload;
actions!(message_edit, [SaveEdit]);
impl Conversation {
    pub(crate) fn can_mutate(&self, id: u64) -> bool {
        self.ready
            && self.mutation_pending.is_none()
            && self.caps.iter().any(|c| c == "edit")
            && self
                .store
                .get(id)
                .is_some_and(|m| !m.deleted() && m.raw["from"]["user_id"] == self.you)
    }
    pub(crate) fn open_mutation(
        &mut self,
        id: u64,
        delete: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_mutate(id) || w.has_active_dialog(cx) {
            return;
        }
        let original = self.store.get(id).unwrap().clone();
        let owner = cx.entity();
        let form = cx.new(|cx| MutationForm::new(owner, original, delete, w, cx));
        self.mutation = Some(form.downgrade());
        w.open_dialog(cx, move |d, _, _| {
            let close = form.clone();
            d.on_close(move |_, _, cx| close.update(cx, |f, _| f.closed = true))
                .title(if delete {
                    "Delete message?"
                } else {
                    "Edit message"
                })
                .child(form.clone())
        });
    }
}
pub struct MutationForm {
    owner: WeakEntity<Conversation>,
    original: Arc<Message>,
    pub(crate) text: Entity<TextareaState>,
    delete: bool,
    operation: Option<u64>,
    closed: bool,
    busy: bool,
    error: String,
    _watch: Subscription,
}
impl MutationForm {
    fn new(
        owner: Entity<Conversation>,
        original: Arc<Message>,
        delete: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = cx.new(|cx| {
            let mut s = TextareaState::new(w, cx).auto_grow(3, 8);
            s.set_value(model::text(&original.raw["body"]["text"]), w, cx);
            s
        });
        let watch = cx.observe(&owner, |_, _, cx| cx.notify());
        Self {
            owner: owner.downgrade(),
            original,
            text,
            delete,
            operation: None,
            closed: false,
            busy: false,
            error: String::new(),
            _watch: watch,
        }
    }
    fn stale(&self, v: &Conversation) -> bool {
        v.store.get(self.original.id).is_none_or(|m| {
            m.log != self.original.log || m.deleted() || m.room != self.original.room
        }) || v.you != self.original.raw["from"]["user_id"]
    }
    pub(crate) fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if self.busy {
            return;
        }
        if self.stale(owner.read(cx)) {
            self.error="This message changed or left the cache. Close and reopen it before making changes.".into();
            cx.notify();
            return;
        }
        if !owner.read(cx).can_mutate(self.original.id) {
            self.error = "The connection or permission state no longer allows this action.".into();
            cx.notify();
            return;
        }
        let value = self.text.read(cx).value();
        let result = payload(&self.original.raw, (!self.delete).then_some(value.as_ref()));
        let params = match result {
            Ok(v) => v,
            Err(e) => {
                self.error = e.to_string();
                cx.notify();
                return;
            }
        };
        owner.update(cx, |v, cx| {
            v.mutation_serial += 1;
            let operation = v.mutation_serial;
            if v.command(
                Command::Mutate {
                    operation,
                    payload: params,
                },
                cx,
            ) {
                v.mutation_pending = Some(operation);
                self.operation = Some(operation);
                self.busy = true;
                self.error.clear();
            }
        });
        cx.notify();
    }
    pub(crate) fn finished(
        &mut self,
        operation: u64,
        result: Result<(), String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.operation != Some(operation) {
            return;
        }
        self.busy = false;
        match result {
            Ok(()) => {
                if !self.closed {
                    self.closed = true;
                    w.close_dialog(cx);
                }
            }
            Err(e) => {
                self.error = e;
                cx.notify();
            }
        }
    }
}
impl Render for MutationForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stale = self.owner.upgrade().is_none_or(|v| self.stale(v.read(cx)));
        let disabled = self.busy || stale || self.owner.upgrade().is_none_or(|v| !v.read(cx).ready);
        div().key_context("MessageEditor").on_action(cx.listener(|this,_:&SaveEdit,w,cx|this.submit(w,cx))).flex().flex_col().gap_3()
   .when(self.delete,|d|d.child(div().text_sm().child("Delete this message from the conversation? Earlier server snapshots may retain its content. This cannot be undone here."))
     .child(div().max_h_24().overflow_hidden().text_color(cx.theme().muted_foreground).child(model::clean(model::text(&self.original.raw["body"]["text"]),180))))
   .when(!self.delete,|d|d.child(Textarea::new(&self.text).aria_label("Edit message text").w_full()).child(div().text_xs().text_color(cx.theme().muted_foreground).child("Ctrl+Enter to save. Existing formatting, mentions, reply reference and attachments are preserved.")))
   .when(stale,|d|d.child(div().text_sm().text_color(cx.theme().danger).child("The message changed. Close and reopen to review the latest version.")))
   .when(!self.error.is_empty() && !stale,|d|d.child(div().text_sm().text_color(cx.theme().danger).child(self.error.clone())))
   .when(self.busy,|d|d.child(div().text_sm().child("Waiting for the server. Closing this dialog does not cancel an already submitted change.")))
   .child(div().flex().justify_end().gap_2().child(crate::design::button("cancel-edit").label(if self.busy{"Close"}else{"Cancel"}).on_click(cx.listener(|f,_,w,cx|{f.closed=true;w.close_dialog(cx);})))
    .child(crate::design::button("save-edit").when(self.delete,|b|b.danger()).when(!self.delete,|b|b.primary()).label(if self.delete{"Delete message"}else{"Save changes"}).disabled(disabled).on_click(cx.listener(|this,_,w,cx|this.submit(w,cx)))))
    }
}
