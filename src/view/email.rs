use super::*;
struct EmailForm {
    owner: WeakEntity<Conversation>,
    session: apron_gpui::email::Session,
    address: Entity<InputState>,
    code: Entity<InputState>,
    proposed: bool,
    busy: bool,
    remember: bool,
    note: String,
}
impl EmailForm {
    fn submit(&mut self, approve: bool, w: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let request = if approve {
            match Secret::new(self.code.read(cx).value().to_string()) {
                Ok(code) => apron_gpui::email::Request::Approve(code),
                Err(_) => {
                    self.note = "Enter the emailed code or token.".into();
                    cx.notify();
                    return;
                }
            }
        } else {
            apron_gpui::email::Request::Propose(self.address.read(cx).value().to_string())
        };
        if self.session.commands.try_send(request).is_err() {
            self.note = "Sign-in session ended. Close and reopen this form.".into();
            cx.notify();
            return;
        }
        self.busy = true;
        self.note = "Contacting server…".into();
        let rx = self.session.results.clone();
        self.code.update(cx, |s, cx| s.set_value("", w, cx));
        cx.notify();
        cx.spawn_in(w, async move |v, cx| {
            let result = rx.recv().await;
            let _ = v.update_in(cx, |v, w, cx| {
                v.busy = false;
                match result {
                    Ok(Ok(Some(token))) => {
                        if let Some(owner) = v.owner.upgrade() {
                            owner.update(cx, |owner, cx| {
                                owner.remember_session = v.remember;
                                owner.change_session(Some(token), w, cx);
                            });
                        }
                        w.close_dialog(cx);
                    }
                    Ok(Ok(None)) => {
                        v.proposed = true;
                        v.note =
                            "If the address is eligible, a code is on its way. Enter it below."
                                .into();
                        v.code.update(cx, |s, cx| s.focus(w, cx));
                    }
                    Ok(Err(e)) => v.note = e,
                    Err(_) => v.note = "Sign-in connection closed. Reopen this form.".into(),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
impl Render for EmailForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::design::surface(cx)
            .child("Email address")
            .child(
                Input::new(&self.address)
                    .aria_label("Email address")
                    .w_full(),
            )
            .child(
                crate::design::button("email-propose")
                    .label(if self.proposed {
                        "Request a new code"
                    } else {
                        "Email me a code"
                    })
                    .disabled(self.busy)
                    .on_click(cx.listener(|v, _, w, cx| v.submit(false, w, cx))),
            )
            .when(self.proposed, |d| {
                d.child("Email code or token")
                    .child(Input::new(&self.code).aria_label("Email code").w_full())
                    .child(
                        crate::design::button("email-approve")
                            .primary()
                            .label("Sign in")
                            .disabled(self.busy)
                            .on_click(cx.listener(|v, _, w, cx| v.submit(true, w, cx))),
                    )
            })
            .child(
                crate::design::button("email-remember")
                    .label(if self.remember {
                        "✓ Remember in keyring"
                    } else {
                        "Remember in keyring (off)"
                    })
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.remember = !v.remember;
                        cx.notify();
                    })),
            )
            .child(div().text_sm().child(self.note.clone()))
    }
}
impl Conversation {
    pub(crate) fn email_dialog(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if w.has_active_dialog(cx) || !self.can_change_session() {
            return;
        }
        let owner = cx.weak_entity();
        let server = self.server.clone();
        let form = cx.new(|cx| {
            let address = cx.new(|cx| InputState::new(w, cx).placeholder("you@example.com"));
            let code = cx.new(|cx| InputState::new(w, cx).masked(true));
            address.update(cx, |s, cx| s.focus(w, cx));
            EmailForm {
                owner,
                session: apron_gpui::email::Session::start(server),
                address,
                code,
                proposed: false,
                busy: false,
                remember: false,
                note: "Account creation and email delivery depend on this server's policy.".into(),
            }
        });
        w.open_dialog(cx, move |d, _, _| {
            d.title("Email sign-in").child(form.clone())
        });
    }
}
