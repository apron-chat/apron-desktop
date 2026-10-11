//! Opt-in synthetic native interaction and visual checkpoints; never a production timer.
use crate::{
    smoke::{key, settle, wait},
    view::Conversation,
};
use apron_gpui::{model, transport::Event};
use gpui_kit::*;
use std::time::Duration;

pub fn start(view: &Entity<Conversation>, output: String, window: &mut Window, cx: &mut App) {
    view.update(cx, |v, cx| {
        v.inspect = true;
        cx.notify();
    });
    view.update(cx, |_, cx| {
        cx.spawn_in(window, async move |view, cx| run(view, output, cx).await)
            .detach();
    });
}
async fn stage(output: &str, name: &str, cx: &AsyncWindowContext) {
    std::fs::write(format!("{output}.stage"), name).unwrap();
    cx.background_executor().timer(Duration::from_secs(2)).await;
}
async fn right_click(position: Point<Pixels>, cx: &mut AsyncWindowContext) {
    cx.update(|w, cx| {
        w.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Right,
                position,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    cx.update(|w, cx| {
        w.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Right,
                position,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
}
async fn run(view: WeakEntity<Conversation>, output: String, cx: &mut AsyncWindowContext) {
    let fixture_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 100_000;
    wait(&view, cx, |v, _| {
        v.ready && !v.loading && v.rows.len() >= 30
    })
    .await;
    settle(cx).await;
    stage(&output, "wide", cx).await;
    cx.update(crate::design::gallery).unwrap();
    settle(cx).await;
    stage(&output, "gallery", cx).await;
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;
    view.update_in(cx, |v, w, cx| {
        v.manage_dialog(apron_gpui::workflows::Kind::Create, w, cx)
    })
    .unwrap();
    settle(cx).await;
    stage(&output, "create-room", cx).await;
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;

    view.update_in(cx, |v, w, cx| v.composer.update(cx, |s, cx| s.focus(w, cx)))
        .unwrap();
    cx.update(|w, cx| key("ctrl-f", w, cx)).unwrap();
    view.update_in(cx, |v, w, cx| {
        v.search
            .update(cx, |s, cx| s.set_value("fixture note", w, cx))
    })
    .unwrap();
    settle(cx).await;
    let first = view
        .update_in(cx, |v, _, cx| {
            assert!(v.searching);
            assert!(v.match_ids(cx).len() > 2);
            v.match_ids(cx)[0]
        })
        .unwrap();
    cx.update(|w, cx| key("f3", w, cx)).unwrap();
    settle(cx).await;
    view.update_in(cx, |v, _, cx| assert_ne!(v.match_ids(cx)[1], first))
        .unwrap();
    stage(&output, "search", cx).await;
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;
    assert!(!view.update_in(cx, |v, _, _| v.searching).unwrap());
    cx.update(|w, cx| key("f3", w, cx)).unwrap();
    assert!(!view.update_in(cx, |v, _, _| v.searching).unwrap());
    view.update_in(cx, |v, w, cx| {
        v.select("design".into(), w, cx);
        assert!(v.match_ids(cx).is_empty());
    })
    .unwrap();
    wait(&view, cx, |v, _| !v.loading).await;
    view.update_in(cx, |v, w, cx| v.select("general".into(), w, cx))
        .unwrap();
    wait(&view, cx, |v, _| !v.loading).await;

    view.update_in(cx, |v, _, cx| {
        v.scroller.update(cx, |s, cx| s.scroll_to_end(cx))
    })
    .unwrap();
    settle(cx).await;

    let (id, bounds) = view
        .update_in(cx, |v, w, _| {
            v.geometry
                .borrow()
                .iter()
                .filter(|(_, b)| {
                    b.top() > px(100.)
                        && b.bottom() < px(600.).min(w.viewport_size().height - px(180.))
                })
                .max_by(|a, b| a.1.top().partial_cmp(&b.1.top()).unwrap())
                .map(|(id, b)| (*id, *b))
                .unwrap()
        })
        .unwrap();
    right_click(bounds.origin + point(px(30.), px(8.)), cx).await;
    stage(&output, "message-menu", cx).await;
    cx.update(|w, cx| key("down", w, cx)).unwrap();
    cx.update(|w, cx| key("enter", w, cx)).unwrap();
    settle(cx).await;
    assert_eq!(
        view.update_in(cx, |v, _, _| v.reply).unwrap(),
        Some(id),
        "Menu reply targeted wrong message"
    );
    stage(&output, "reply", cx).await;
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    assert_eq!(view.update_in(cx, |v, _, _| v.reply).unwrap(), None);
    // Cancelling the reply resizes the composer; resolve this row after layout settles.
    settle(cx).await;
    let bounds = view
        .update_in(cx, |v, _, _| *v.geometry.borrow().get(&id).unwrap())
        .unwrap();
    // Right-click whole-message copy uses the same scoped action as the keyboard menu.
    right_click(bounds.origin + point(px(30.), px(8.)), cx).await;
    for k in ["down", "down", "enter"] {
        cx.update(|w, cx| key(k, w, cx)).unwrap();
    }
    settle(cx).await;
    let expected = view
        .update_in(cx, |v, _, _| {
            model::clean(
                model::text(&v.store.get(id).unwrap().raw["body"]["text"]),
                32000,
            )
        })
        .unwrap();
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
            .unwrap(),
        Some(expected)
    );
    // Exercise the native nested reaction menu, then toggle off via the same action.
    let bounds = view
        .update_in(cx, |v, _, _| *v.geometry.borrow().get(&id).unwrap())
        .unwrap();
    right_click(bounds.origin + point(px(30.), px(8.)), cx).await;
    for k in ["down", "down", "down", "down", "right", "enter"] {
        cx.update(|w, cx| key(k, w, cx)).unwrap();
    }
    wait(&view, cx, |v, _| !v.reactions.totals(id).is_empty()).await;
    stage(&output, "reactions", cx).await;
    view.update_in(cx, |v, _, cx| v.react(id, "👍", cx))
        .unwrap();
    wait(&view, cx, |v, _| v.reactions.totals(id).is_empty()).await;
    // Deliberately clip the first row header before keyboard traversal.
    view.update_in(cx, |v, _, cx| {
        v.scroller.update(cx, |s, cx| {
            s.scroll_to_item(0, cx);
        });
    })
    .unwrap();
    settle(cx).await;
    cx.update(|w, cx| {
        w.dispatch_event(
            ScrollWheelEvent {
                position: point(px(350.), px(220.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-32.))),
                touch_phase: TouchPhase::Moved,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        )
    })
    .unwrap();
    settle(cx).await;
    stage(&output, "partial-before-focus", cx).await;
    cx.update(|w, cx| key("ctrl-k", w, cx)).unwrap();
    settle(cx).await;
    assert!(
        cx.update(|w, cx| {
            use gpui_kit::component::WindowExt;
            w.has_active_dialog(cx)
        })
        .unwrap()
    );
    stage(&output, "switcher", cx).await;
    let switcher = view
        .update_in(cx, |v, _, _cx| {
            v.switcher_form.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    cx.update(|w, cx| {
        switcher.update(cx, |v, cx| {
            v.input.update(cx, |s, cx| s.set_value("eng", w, cx))
        })
    })
    .unwrap();
    settle(cx).await;
    assert_eq!(
        cx.update(|_, cx| switcher.read(cx).results(cx)[0].0.clone())
            .unwrap(),
        "engineering"
    );
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;
    if !view.update_in(cx, |v, w, _| v.sidebar_visible(w)).unwrap() {
        cx.update(|w, cx| key("ctrl-b", w, cx)).unwrap();
        settle(cx).await;
    }
    view.update_in(cx, |v, w, cx| v.filter.update(cx, |s, cx| s.focus(w, cx)))
        .unwrap();

    view.update_in(cx, |v, _, _| *v.focused_row.borrow_mut() = None)
        .unwrap();
    let mut reached_message = false;
    for _ in 0..40 {
        cx.update(|w, cx| key("tab", w, cx)).unwrap();
        settle(cx).await;
        if cx
            .update(|w, _| w.context_stack().iter().any(|c| c.contains("Message")))
            .unwrap()
        {
            reached_message = true;
            break;
        }
    }
    assert!(reached_message, "Tab never reached message actions");
    stage(&output, "keyboard-message-focus", cx).await;
    view.update_in(cx, |v, _, _| {
        let (_, bounds) = v.focused_row.borrow().expect("Focused message geometry");
        let viewport = *v.transcript_bounds.borrow();
        assert!(
            bounds.top() >= viewport.top() && bounds.top() + px(32.) <= viewport.bottom(),
            "Focused message actions clipped: {bounds:?} in {viewport:?}"
        );
    })
    .unwrap();
    cx.update(|w, cx| key("r", w, cx)).unwrap();
    settle(cx).await;
    assert!(
        view.update_in(cx, |v, _, _| v.reply.is_some()).unwrap(),
        "Keyboard-only reply failed"
    );
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;
    view.update_in(cx, |v, w, cx| v.filter.update(cx, |s, cx| s.focus(w, cx)))
        .unwrap();
    settle(cx).await;
    view.update_in(cx, |v, w, cx| {
        assert!(v.sidebar_visible(w));
        assert!(v.filter.read(cx).focus_handle(cx).is_focused(w));
        v.filter
            .update(cx, |s, cx| s.set_value("no-such-synthetic-room", w, cx));
    })
    .unwrap();
    stage(&output, "filter-empty", cx).await;
    view.update_in(cx,|v,w,cx| {v.filter.update(cx,|s,cx|s.set_value("",w,cx));
        v.event(Event::Notice(serde_json::json!({"method":"message","params":{"message_id":fixture_id.to_string(),"log_id":fixture_id.to_string(),"room_id":"design","from":{"user_id":"synthetic-ada","name":"Ada"},"body":{"text":"A synthetic background mention.","mentions":["demo_you"]}}})),w,cx);
        assert_eq!(v.attention.counts("design"),(1,1));
    }).unwrap();
    view.update_in(cx, |v, _, cx| {
        v.scroller.update(cx, |s, cx| s.scroll_to_item(0, cx))
    })
    .unwrap();
    settle(cx).await;
    view.update_in(cx, |v,w,cx| {
        v.event(Event::Notice(serde_json::json!({"method":"message","params":{"message_id":(fixture_id+2).to_string(),"log_id":(fixture_id+2).to_string(),"room_id":"general","from":{"user_id":"synthetic-ada","name":"Ada"},"body":{"text":"A synthetic message received while reading earlier history."}}})),w,cx);
        assert_eq!(v.attention.counts("general"),(1,0),"Reading earlier history must not clear new messages");
    }).unwrap();
    stage(&output, "unread", cx).await;
    cx.update(|w, cx| key("ctrl-shift-u", w, cx)).unwrap();
    settle(cx).await;
    view.update_in(cx, |v, _, _| {
        assert_eq!(v.attention.counts("general"), (0, 0))
    })
    .unwrap();
    view.update_in(cx, |v, _, cx| {
        v.scroller.update(cx, |s, cx| s.scroll_to_end(cx))
    })
    .unwrap();
    settle(cx).await;
    // Open the actual toolbar menu, then activate its session action by keyboard.
    let position = cx
        .update(|w, _| point(w.viewport_size().width - px(36.), px(32.)))
        .unwrap();
    cx.update(|w, cx| {
        w.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    cx.update(|w, cx| {
        w.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    stage(&output, "conversation-menu", cx).await;
    for k in ["down", "down", "down", "down", "enter"] {
        cx.update(|w, cx| key(k, w, cx)).unwrap();
    }
    settle(cx).await;
    view.update_in(cx, |v, _, _| {
        assert!(
            v.session_form.as_ref().and_then(|f| f.upgrade()).is_some(),
            "Toolbar menu did not open session dialog"
        )
    })
    .unwrap();
    stage(&output, "sign-in", cx).await;
    // Escape from the masked field must dismiss the dialog, not cancel a background reply.
    let form = view
        .update_in(cx, |v, w, cx| {
            let f = v.session_form.as_ref().unwrap().upgrade().unwrap();
            f.update(cx, |f, cx| f.token.update(cx, |s, cx| s.focus(w, cx)));
            f.downgrade()
        })
        .unwrap();
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;
    assert!(
        form.upgrade().is_none(),
        "Escape did not release the dialog form"
    );
    view.update_in(cx, |v, w, cx| v.session_dialog(w, cx))
        .unwrap();
    settle(cx).await;
    view.update_in(cx, |v, w, cx| {
        v.session_form
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .update(cx, |f, cx| {
                f.token.update(cx, |s, cx| {
                    s.set_value("synthetic-invalid-token", w, cx);
                    s.focus(w, cx)
                })
            })
    })
    .unwrap();
    stage(&output, "token-masked", cx).await;
    cx.update(|w, cx| key("enter", w, cx)).unwrap();
    wait(&view, cx, |v, _| {
        !v.ready && v.error.contains("Sign-in was not accepted")
    })
    .await;
    stage(&output, "sign-in-error", cx).await;
    view.update_in(cx, |v, w, cx| v.session_dialog(w, cx))
        .unwrap();
    settle(cx).await;
    view.update_in(cx, |v, w, cx| {
        v.session_form
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .update(cx, |f, cx| {
                f.token.update(cx, |s, cx| {
                    s.set_value("local-fixture-token", w, cx);
                    s.focus(w, cx)
                })
            })
    })
    .unwrap();
    cx.update(|w, cx| key("enter", w, cx)).unwrap();
    wait(&view, cx, |v, _| v.ready && v.authenticated && !v.loading).await;
    view.update_in(cx, |v, w, cx| v.select("design".into(), w, cx))
        .unwrap();
    wait(&view, cx, |v, _| !v.loading).await;
    stage(&output, "empty-room", cx).await;
    view.update_in(cx,|v,w,cx|v.event(Event::Notice(serde_json::json!({"method":"message","params":{"message_id":(fixture_id+1).to_string(),"log_id":(fixture_id+1).to_string(),"room_id":"design","from":{"user_id":"synthetic-ada","name":"Ada with a deliberately long synthetic display name"},"body":{"format":"markdown","text":format!("# Long-message layout\n\n{}\n\n```rust\nfn example() {{ println!(\"synthetic only\"); }}\n```", "A long paragraph should wrap inside the conversation without widening the window. ".repeat(18))}}})),w,cx)).unwrap();
    settle(cx).await;
    stage(&output, "long-message", cx).await;
    view.update_in(cx, |v, w, cx| v.select("general".into(), w, cx))
        .unwrap();
    wait(&view, cx, |v, _| !v.loading).await;
    cx.update(|w, cx| {
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(w), cx)
    })
    .unwrap();
    settle(cx).await;
    stage(&output, "dark", cx).await;
    cx.update(crate::design::gallery).unwrap();
    settle(cx).await;
    stage(&output, "gallery-dark", cx).await;
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;
    cx.update(|_, cx| gpui_kit::component::Theme::update(cx, |theme| theme.font_size = px(20.)))
        .unwrap();
    settle(cx).await;
    stage(&output, "large-text", cx).await;
    let report = serde_json::json!({"loaded_search_keyboard":true,"reaction_menu_add_remove":true,"native_wayland":true,"context_menu_reply_target":true,"context_menu_copy":true,"keyboard_menu_navigation":true,"toolbar_session_menu":true,"tab_reaches_message_actions":true,"keyboard_only_reply":true,"escape_cancels_reply":true,"ctrl_k_fuzzy_switcher":true,"background_unread_mention":true,"history_reading_preserves_unread":true,"keyboard_mark_read":true,"escape_dismisses_masked_token_dialog":true,"rejected_token_error":true,"valid_token_session":true,"synthetic_data_only":true});
    std::fs::write(&output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    cx.update(|_, cx| cx.quit()).unwrap();
}
