//! Short-lived masked token form. No file, preferences, environment or CLI token path.
use crate::view::Conversation;
use apron_gpui::auth::Secret;
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    button::ButtonVariants,
    input::{Input, InputState},
};
use gpui_kit::{prelude::*, *};
actions!(session, [SubmitSession]);
pub struct SessionForm {
    pub(crate) token: Entity<InputState>,
    owner: WeakEntity<Conversation>,
    server: String,
    address: Entity<InputState>,
    error: String,
    remember: bool,
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
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("wss://server/ws"));
        address.update(cx, |s, cx| s.set_value(server.clone(), window, cx));
        Self {
            address,
            token,
            owner,
            server,
            error: String::new(),
            remember: false,
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
        let server = self.address.read(cx).value().to_string();
        if apron_gpui::media::endpoint(&server).is_err() {
            self.error = "Use a valid wss:// address, or ws:// on loopback.".into();
            cx.notify();
            return;
        }
        self.clear(window, cx);
        owner.update(cx, |v, cx| {
            v.remember_session = self.remember;
            v.connect_to(server, Some(token), window, cx);
        });
        window.close_dialog(cx);
    }
    pub(crate) fn actions(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let actions = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                crate::design::button("cancel-session")
                    .label("Cancel")
                    .on_click(|_, w, cx| w.close_dialog(cx)),
            )
            .child(
                crate::design::button("guest-session")
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
                        let server = this.address.read(cx).value().to_string();
                        if apron_gpui::media::endpoint(&server).is_err() {
                            this.error = "Enter a valid server address.".into();
                            cx.notify();
                            return;
                        }
                        this.clear(w, cx);
                        owner.update(cx, |v, cx| {
                            v.remember_session = false;
                            v.connect_to(server, None, w, cx);
                        });
                        w.close_dialog(cx);
                    })),
            )
            .child(
                crate::design::button("sign-in")
                    .primary()
                    .label("Sign in")
                    .on_click(cx.listener(|this, _, w, cx| this.submit(w, cx))),
            );
        actions.into_any_element()
    }
}
impl Render for SessionForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let field = div()
            .flex()
            .flex_col()
            .gap_2()
            .child("Server address")
            .child(
                Input::new(&self.address)
                    .aria_label("Server address")
                    .w_full(),
            )
            .child("Access token")
            .child(
                Input::new(&self.token)
                    .id("session-token")
                    .aria_label("Access token")
                    .w_full(),
            );
        let field = field
            .child(
                crate::design::button("remember-token")
                    .label(if self.remember {
                        "✓ Remember in system keyring"
                    } else {
                        "Remember in system keyring (off)"
                    })
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.remember = !v.remember;
                        cx.notify();
                    })),
            )
            .child(
                crate::design::button("load-token")
                    .label("Use saved keyring session")
                    .on_click(cx.listener(|v, _, w, cx| {
                        if let Some(owner) = v.owner.upgrade() {
                            if !owner.read(cx).can_change_session() {
                                v.error = "Finish the current operation first.".into();
                                cx.notify();
                                return;
                            }
                            let server = v.address.read(cx).value().to_string();
                            if apron_gpui::media::endpoint(&server).is_err() {
                                v.error = "Enter a valid server address.".into();
                                cx.notify();
                                return;
                            }
                            v.clear(w, cx);
                            owner.update(cx, |v, cx| {
                                v.remember_session = true;
                                v.connect_to(server, None, w, cx);
                            });
                            w.close_dialog(cx);
                        }
                    })),
            )
            .child(
                crate::design::button("forget-token")
                    .label("Forget saved keyring session")
                    .on_click(cx.listener(|v, _, w, cx| {
                        let server = v.address.read(cx).value().to_string();
                        if apron_gpui::media::endpoint(&server).is_err() {
                            v.error = "Enter a valid server address.".into();
                            cx.notify();
                            return;
                        }
                        v.error = "Removing saved keyring session…".into();
                        cx.notify();
                        cx.spawn_in(w, async move |v, cx| {
                            let result = cx
                                .background_executor()
                                .spawn(async move {
                                    tokio::runtime::Runtime::new()
                                        .unwrap()
                                        .block_on(apron_gpui::vault::clear(&server))
                                        .map_err(|e| e.to_string())
                                })
                                .await;
                            let _ = v.update(cx, |v, cx| {
                                v.error = result
                                    .err()
                                    .unwrap_or_else(|| "Saved session removed.".into());
                                cx.notify();
                            });
                        })
                        .detach();
                    })),
            );
        div().key_context("Session")
            .on_action(cx.listener(|this, _: &SubmitSession, w, cx| this.submit(w, cx)))
            .flex().flex_col().gap_4()
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(self.server.clone()))
            .child(field)
            .when(!self.error.is_empty(), |d| d.child(div().text_sm().text_color(cx.theme().danger).child(self.error.clone())))
            .child(div().text_sm().child("Tokens stay in memory unless you opt into the system keyring. There is no plaintext-file fallback."))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Changing sessions clears current drafts and selected files. Email sign-in is available from the conversation menu."))
    }
}
