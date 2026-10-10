//! Short-lived masked token form. No file, preferences, environment or CLI token path.
use crate::view::Conversation;
use apron_gpui::auth::Secret;
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
};
use gpui_kit::{prelude::*, *};
actions!(session, [SubmitSession]);
pub struct SessionForm {
    pub(crate) token: Entity<InputState>,
    owner: WeakEntity<Conversation>,
    server: String,
    error: String,
}
impl SessionForm {
    pub fn new(
        owner: WeakEntity<Conversation>,
        server: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let token = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Paste your access token")
        });
        Self {
            token,
            owner,
            server,
            error: String::new(),
        }
    }
    pub(crate) fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.token.update(cx, |s, cx| s.set_value("", window, cx));
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let token = match Secret::new(self.token.read(cx).value().to_string()) {
            Ok(t) => t,
            Err(_) => {
                self.error = "Enter a token of 1–8192 bytes without control characters.".into();
                cx.notify();
                return;
            }
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if !owner.read(cx).can_change_session() {
            self.error = "Finish the current send or cancel the transfer before signing in.".into();
            cx.notify();
            return;
        }
        self.clear(window, cx);
        owner.update(cx, |v, cx| v.change_session(Some(token), window, cx));
        window.close_dialog(cx);
    }
}
impl Render for SessionForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let field = div().flex().flex_col().gap_2().child("Access token").child(
            Input::new(&self.token)
                .id("session-token")
                .aria_label("Access token")
                .w_full(),
        );
        let actions = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("cancel-session")
                    .label("Cancel")
                    .on_click(|_, w, cx| w.close_dialog(cx)),
            )
            .child(
                Button::new("guest-session")
                    .label("Use guest")
                    .on_click(cx.listener(|this, _, w, cx| {
                        let Some(owner) = this.owner.upgrade() else {
                            return;
                        };
                        if !owner.read(cx).can_change_session() {
                            this.error =
                                "Finish the current send or cancel the transfer first.".into();
                            cx.notify();
                            return;
                        }
                        this.clear(w, cx);
                        owner.update(cx, |v, cx| v.change_session(None, w, cx));
                        w.close_dialog(cx);
                    })),
            )
            .child(
                Button::new("sign-in")
                    .primary()
                    .label("Sign in")
                    .on_click(cx.listener(|this, _, w, cx| this.submit(w, cx))),
            );
        div().key_context("Session")
            .on_action(cx.listener(|this, _: &SubmitSession, w, cx| this.submit(w, cx)))
            .flex().flex_col().gap_4()
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(self.server.clone()))
            .child(field)
            .when(!self.error.is_empty(), |d| d.child(div().text_sm().text_color(cx.theme().danger).child(self.error.clone())))
            .child(div().text_sm().child("Your token stays in memory for this session, including reconnects. It is never saved to disk."))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Changing sessions clears room drafts and selected files. Use a token from your server; this client does not create accounts."))
            .child(actions)
    }
}
