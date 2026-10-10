//! Drives the real Wayland window with native GPUI pointer/key events.
use crate::view::Conversation;
use apron_gpui::transport::Event;
use gpui_kit::base::TextSelection;
use gpui_kit::*;
use std::time::{Duration, Instant};
async fn settle(cx: &AsyncWindowContext) {
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
}
async fn wait(
    view: &WeakEntity<Conversation>,
    cx: &mut AsyncWindowContext,
    predicate: impl Fn(&Conversation, &App) -> bool,
) {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if view.update_in(cx, |v, _, cx| predicate(v, cx)).unwrap() {
            return;
        }
        assert!(Instant::now() < end, "Native workflow timeout");
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
}
fn key(key: &str, window: &mut Window, cx: &mut App) {
    window.dispatch_event(
        KeyDownEvent {
            keystroke: Keystroke::parse(key).unwrap(),
            is_held: false,
            prefer_character_input: false,
        }
        .to_platform_input(),
        cx,
    );
}
pub fn start(view: &Entity<Conversation>, output: String, window: &mut Window, cx: &mut App) {
    view.update(cx, |view, cx| {
        view.inspect = true;
        cx.notify();
    });
    view.update(cx, |_, cx| {
        cx.spawn_in(window, async move |view, cx| run(view, output, cx).await)
            .detach();
    });
}
async fn run(view: WeakEntity<Conversation>, output: String, cx: &mut AsyncWindowContext) {
    wait(&view, cx, |v, _| {
        v.ready && !v.loading && v.rows.len() >= 30
    })
    .await;
    view.update_in(cx, |v, _, cx| {
        v.scroller.update(cx, |s, cx| {
            s.scroll_to_item(10, cx);
        })
    })
    .unwrap();
    settle(cx).await;
    let (id, bounds) = view
        .update_in(cx, |v, _, _| {
            v.geometry
                .borrow()
                .iter()
                .filter(|(_, b)| b.top() > px(110.) && b.bottom() < px(550.))
                .min_by(|a, b| a.1.top().partial_cmp(&b.1.top()).unwrap())
                .map(|(id, b)| (*id, *b))
                .expect("Visible text geometry")
        })
        .unwrap();
    let start = bounds.origin + point(px(4.), px(8.));
    let end = start + point(px(150.), px(0.));
    eprintln!("Selection target {id}: {bounds:?}");
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position: start,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    let selected = cx.update(TextSelection::selected_text).unwrap();
    assert!(!selected.trim().is_empty(), "Native drag selected no text");
    cx.update(|window, cx| key("ctrl-c", window, cx)).unwrap();
    settle(cx).await;
    eprintln!("Selection passed: {selected:?}");
    let clipboard = view
        .update_in(cx, |_, _, cx| {
            cx.read_from_clipboard().and_then(|c| c.text())
        })
        .unwrap();
    // GPUI Kit intentionally trims selection edges when copying.
    assert_eq!(
        clipboard.as_deref(),
        Some(selected.trim()),
        "Copy selection"
    );
    let external_clipboard = if std::env::var_os("APRON_VERIFY_CLIPBOARD").is_some() {
        let result = cx
            .background_executor()
            .spawn(async move {
                std::process::Command::new("timeout")
                    .args(["5", "wl-paste", "--no-newline"])
                    .output()
            })
            .await
            .expect("wl-paste installed for compositor clipboard verification");
        assert!(
            result.status.success(),
            "Wayland clipboard reader failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = String::from_utf8(result.stdout).unwrap();
        assert_eq!(text, selected.trim(), "Inter-process Wayland clipboard");
        Some(true)
    } else {
        None
    };
    view.update_in(cx, |v, _, cx| v.older(cx)).unwrap();
    wait(&view, cx, |v, _| !v.loading).await;
    settle(cx).await;
    let anchor_delta = view
        .update_in(cx, |v, _, _| {
            let after = *v
                .geometry
                .borrow()
                .get(&id)
                .expect("Anchor remains rendered");
            f32::from(after.top() - bounds.top()).abs()
        })
        .unwrap();
    assert!(
        anchor_delta < 2.,
        "Prepend shifted anchor by {anchor_delta}"
    );
    assert_eq!(
        cx.update(TextSelection::selected_text).unwrap(),
        selected,
        "Selection survives history prepend"
    );
    eprintln!("Anchor passed: {anchor_delta}");
    view.update_in(cx,|v,w,cx|{
  let count=v.store.len();v.event(Event::History{generation:v.generation.wrapping_sub(1),room:v.room.clone(),page:serde_json::json!({"messages":[{"message_id":"9999999999999","log_id":"9999999999999","room_id":"general","body":{"text":"STALE RESPONSE"}}]})},w,cx);assert_eq!(v.store.len(),count);
  v.select("engineering".into(),w,cx);v.select("native".into(),w,cx);v.select("general".into(),w,cx);
 }).unwrap();
    wait(&view, cx, |v, _| !v.loading && v.room == "general").await;
    eprintln!("Room switches passed");
    view.update_in(cx, |v, window, cx| {
        v.smoke_reply(id, window, cx);
        v.composer
            .update(cx, |s, cx| s.set_value("Synthetic room draft", window, cx));
    })
    .unwrap();
    settle(cx).await;
    cx.update(|window, cx| key("escape", window, cx)).unwrap();
    settle(cx).await;
    assert!(
        view.update_in(cx, |v, _, _| v.reply.is_none()).unwrap(),
        "Escape cancels reply"
    );
    let navigation = view.update_in(cx, |v, w, _| v.sidebar_visible(w)).unwrap();
    cx.update(|w, cx| key("ctrl-b", w, cx)).unwrap();
    settle(cx).await;
    assert_ne!(
        view.update_in(cx, |v, w, _| v.sidebar_visible(w)).unwrap(),
        navigation
    );
    cx.update(|w, cx| key("ctrl-b", w, cx)).unwrap();
    settle(cx).await;
    view.update_in(cx, |v, w, cx| {
        v.select("engineering".into(), w, cx);
        v.select("general".into(), w, cx);
    })
    .unwrap();
    wait(&view, cx, |v, _| !v.loading).await;
    assert_eq!(
        view.update_in(cx, |v, _, cx| v.composer.read(cx).value().to_string())
            .unwrap(),
        "Synthetic room draft"
    );
    view.update_in(cx, |v, window, cx| {
        v.smoke_reply(id, window, cx);
        v.composer.update(cx, |s, cx| {
            s.set_value("Synthetic GPUI native workflow", window, cx)
        });
    })
    .unwrap();
    settle(cx).await;
    cx.update(|window, cx| key("ctrl-enter", window, cx))
        .unwrap();
    wait(&view, cx, |v, cx| {
        v.rows.iter().any(|m| {
            m.raw["body"]["text"] == "Synthetic GPUI native workflow"
                && apron_gpui::model::number(&m.raw["reply_to"]["message_id"]) == Some(id)
        }) && v.composer.read(cx).value().is_empty()
    })
    .await;
    eprintln!("Keyboard reply passed");
    let denied_generation = view
        .update_in(cx, |v, w, cx| {
            v.composer
                .update(cx, |s, cx| s.set_value("/test-denied", w, cx));
            v.send(w, cx);
            v.generation
        })
        .unwrap();
    wait(&view, cx, |v, _| {
        v.error.contains("Fictional permission error")
    })
    .await;
    view.update_in(cx, |v, _, cx| {
        assert!(
            v.ready,
            "Permission error must not reconnect or discard identity"
        );
        assert_eq!(v.generation, denied_generation);
        assert_eq!(v.composer.read(cx).value().as_ref(), "/test-denied");
    })
    .unwrap();
    view.update_in(cx, |v, w, cx| {
        v.composer
            .update(cx, |s, cx| s.set_value("/test-disconnect", w, cx));
        v.send(w, cx);
        v.composer
            .update(cx, |s, cx| s.set_value("Synthetic retained draft", w, cx));
    })
    .unwrap();
    let old = view.update_in(cx, |v, _, _| v.generation).unwrap();
    wait(&view, cx, |v, _| {
        v.generation > old && v.ready && !v.loading
    })
    .await;
    let preserved = view
        .update_in(cx, |v, _, cx| v.composer.read(cx).value().to_string())
        .unwrap();
    assert_eq!(preserved, "Synthetic retained draft");
    let report = serde_json::json!({"native_wayland":true,"drag_selection":selected,"clipboard_matches_trimmed_selection":clipboard.as_deref()==Some(selected.trim()),"external_wayland_clipboard":external_clipboard,"prepend_anchor_delta_px":anchor_delta,"selection_survives_prepend":true,"stale_response_rejected":true,"rapid_room_switches":true,"keyboard_reply_sent":true,"escape_cancels_reply":true,"keyboard_navigation_toggle":true,"room_draft_restored":true,"permission_denied_retains_connection_and_draft":true,"interrupted_send_no_resend_and_draft_retained":true});
    std::fs::write(output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("{report}");
    cx.update(|_, cx| cx.quit()).unwrap();
}
