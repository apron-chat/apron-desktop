//! Explicit opt-in native checks against the isolated official loopback server.
use super::*;
use crate::smoke::{key, settle, wait};
use serde_json::json;
pub fn start(view: &Entity<Conversation>, output: String, window: &mut Window, cx: &mut App) {
    view.update(cx, |v, cx| {
        v.inspect = true;
        cx.spawn_in(window, async move |v, cx| run(v, output, cx).await)
            .detach();
    });
}
async fn stage(output: &str, name: &str, cx: &AsyncWindowContext) {
    std::fs::write(format!("{output}.stage"), name).unwrap();
    cx.background_executor().timer(Duration::from_secs(2)).await;
}
async fn clipboard(bytes: Vec<u8>, mime: &'static str, cx: &AsyncWindowContext) {
    cx.background_executor()
        .spawn(async move {
            use std::io::Write;
            let mut child = std::process::Command::new("wl-copy")
                .args(["--type", mime])
                .stdin(std::process::Stdio::piped())
                .spawn()
                .expect("wl-copy required for real native clipboard check");
            child.stdin.take().unwrap().write_all(&bytes).unwrap();
            assert!(child.wait().unwrap().success());
        })
        .await;
    settle(cx).await;
}
async fn run(view: WeakEntity<Conversation>, output: String, cx: &mut AsyncWindowContext) {
    wait(&view, cx, |v, _| v.ready && !v.loading && !v.local_loading).await;
    view.update_in(cx,|v,w,cx| { v.composer.update(cx,|s,cx|{s.set_value("## A native home for Apron\n\n**Clear text**, *thoughtful details*, and `safe code`.\n\n- Keyboard-friendly conversation\n- Explicit image loading\n\n> Fictional local test data only.",w,cx);s.focus(w,cx);});v.send(w,cx); }).unwrap();
    wait(&view, cx, |v, _| {
        !v.pending
            && v.rows
                .iter()
                .any(|m| model::text(&m.raw["body"]["text"]).starts_with("## A native"))
    })
    .await;
    let id = view
        .update_in(cx, |v, _, _| {
            v.rows
                .iter()
                .find(|m| {
                    m.raw["from"]["user_id"] == v.you
                        && model::text(&m.raw["body"]["text"]).starts_with("## A native")
                })
                .unwrap()
                .id
        })
        .unwrap();
    stage(&output, "markdown", cx).await;
    view.update_in(cx, |v, w, cx| v.open_mutation(id, false, w, cx))
        .unwrap();
    stage(&output, "edit", cx).await;
    let form = view
        .update_in(cx, |v, _, _| {
            v.mutation.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    cx.update(|w, cx| {
        form.update(cx, |f, cx| {
            f.text.update(cx, |s, cx| {
                s.set_value(
                    "## Edited locally\n\n**Markdown**, mentions and images — on Wayland.",
                    w,
                    cx,
                )
            });
            f.submit(w, cx);
        });
    })
    .unwrap();
    wait(&view, cx, |v, _| {
        v.mutation_pending.is_none()
            && v.store
                .get(id)
                .is_some_and(|m| model::text(&m.raw["body"]["text"]).starts_with("## Edited"))
    })
    .await;
    view.update_in(cx, |v, w, cx| v.open_mutation(id, false, w, cx))
        .unwrap();
    let stale_form = view
        .update_in(cx, |v, _, _| {
            v.mutation.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    view.update_in(cx, |v, _, cx| {
        let mut raw = v.store.get(id).unwrap().raw.clone();
        raw["log_id"] = (v.store.get(id).unwrap().log + 1).to_string().into();
        v.store.apply(raw);
        v.refresh(cx);
        cx.notify();
    })
    .unwrap();
    cx.update(|w, cx| stale_form.update(cx, |f, cx| f.submit(w, cx)))
        .unwrap();
    assert!(
        view.update_in(cx, |v, _, _| v.mutation_pending.is_none())
            .unwrap(),
        "Stale edit must not submit"
    );
    stage(&output, "stale-edit", cx).await;
    cx.update(|w, cx| key("escape", w, cx)).unwrap();
    settle(cx).await;
    for _ in 0..2 {
        view.update_in(cx, |v, w, cx| v.open_mutation(id, false, w, cx))
            .unwrap();
        settle(cx).await;
        cx.update(|w, cx| key("escape", w, cx)).unwrap();
        settle(cx).await;
    }
    // Normal native clipboard text must keep the established paste behavior.
    clipboard(
        b"Native clipboard text".to_vec(),
        "text/plain;charset=utf-8",
        cx,
    )
    .await;
    view.update_in(cx, |v, w, cx| {
        v.composer.update(cx, |s, cx| {
            s.set_value("", w, cx);
            s.focus(w, cx);
        })
    })
    .unwrap();
    cx.update(|w, cx| key("ctrl-v", w, cx)).unwrap();
    settle(cx).await;
    assert_eq!(
        view.update_in(cx, |v, _, cx| v.composer.read(cx).value().to_string())
            .unwrap(),
        "Native clipboard text"
    );
    view.update_in(cx, |v, w, cx| {
        v.composer.update(cx, |s, cx| {
            s.set_value("@", w, cx);
            s.set_selected_range(1..1, cx);
            s.focus(w, cx);
        })
    })
    .unwrap();
    settle(cx).await;
    stage(&output, "mentions", cx).await;
    let member = view
        .update_in(cx, |v, _, cx| {
            v.mention_candidates(cx)
                .first()
                .cloned()
                .expect("Actual room member available")
        })
        .unwrap();
    cx.update(|w, cx| key("enter", w, cx)).unwrap();
    settle(cx).await;
    assert!(
        view.update_in(cx, |v, _, cx| apron_gpui::mentions::recipients(
            v.composer.read(cx).value().as_ref(),
            &v.draft_mentions[&v.room]
        )
        .contains(&member.id))
            .unwrap()
    );
    // Decode actual PNG and JPEG MIME offers from a separate Wayland client.
    for (format, mime) in [
        (image::ImageFormat::Png, "image/png"),
        (image::ImageFormat::Jpeg, "image/jpeg"),
    ] {
        let mut raster = image::RgbImage::new(480, 240);
        for (x, y, p) in raster.enumerate_pixels_mut() {
            *p = image::Rgb([40 + (x * 90 / 480) as u8, 90 + (y * 95 / 240) as u8, 165]);
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(raster)
            .write_to(&mut bytes, format)
            .unwrap();
        clipboard(bytes.into_inner(), mime, cx).await;
        let before = view.update_in(cx, |v, _, _| v.files.len()).unwrap();
        view.update_in(cx, |v, w, cx| v.composer.update(cx, |s, cx| s.focus(w, cx)))
            .unwrap();
        cx.update(|w, cx| key("ctrl-v", w, cx)).unwrap();
        wait(&view, cx, move |v, _| v.files.len() == before + 1).await;
        assert!(
            !view.update_in(cx, |v, _, _| v.pending).unwrap(),
            "Paste must never send"
        );
    }
    stage(&output, "image-preview", cx).await;
    let caps = view
        .update_in(cx, |v, _, _| {
            let caps = v.caps.clone();
            v.caps.retain(|c| c != "embed:upload");
            caps
        })
        .unwrap();
    cx.update(|w, cx| key("ctrl-v", w, cx)).unwrap();
    settle(cx).await;
    assert!(
        view.update_in(cx, |v, _, _| v.error.contains("does not support")
            && v.files.len() == 2)
            .unwrap()
    );
    stage(&output, "unsupported-upload", cx).await;
    view.update_in(cx, |v, _, cx| {
        v.caps = caps;
        v.error.clear();
        cx.notify();
    })
    .unwrap();
    view.update_in(cx, |v, w, cx| v.send(w, cx)).unwrap();
    wait(&view, cx, |v, _| {
        !v.pending
            && !v.transfer_busy
            && v.rows.iter().any(|m| {
                m.raw["body"]["embeds"]
                    .as_array()
                    .is_some_and(|e| e.len() == 2 && e[0]["url"].is_string())
            })
    })
    .await;
    let (image_id, key, url) = view
        .update_in(cx, |v, _, _| {
            let m = v
                .rows
                .iter()
                .find(|m| {
                    m.raw["from"]["user_id"] == v.you
                        && m.raw["body"]["embeds"]
                            .as_array()
                            .is_some_and(|e| e.len() == 2 && e[0]["url"].is_string())
                })
                .unwrap();
            let url = m.raw["body"]["embeds"][0]["url"]
                .as_str()
                .unwrap()
                .to_owned();
            (m.id, Conversation::image_key(m.id, 0, &url), url)
        })
        .unwrap();
    assert!(
        view.update_in(cx, |v, _, _| v.image_cache.is_empty())
            .unwrap(),
        "Embedded images must not auto-load"
    );
    let cachekey = key.clone();
    view.update_in(cx, |v, _, cx| {
        v.command(
            Command::LoadImage {
                generation: v.generation,
                key,
                url,
            },
            cx,
        );
    })
    .unwrap();
    wait(&view, cx, move |v, _| v.image_cache.contains_key(&cachekey)).await;
    stage(&output, "image-loaded", cx).await;
    view.update_in(cx, |v, w, cx| {
        v.composer
            .update(cx, |s, cx| s.set_value("Synthetic private draft", w, cx));
        v.enable_local(cx);
        v.privacy_dialog(w, cx);
    })
    .unwrap();
    wait(&view, cx, |v, _| v.local_status == "Saved locally").await;
    stage(&output, "privacy", cx).await;
    view.update_in(cx, |v, w, cx| {
        v.forget_local(true, w, cx);
        w.close_dialog(cx);
    })
    .unwrap();
    wait(&view, cx, |v, _| v.local_status.contains("erased")).await;
    assert!(
        view.update_in(cx, |v, _, cx| !v.local_enabled
            && v.composer.read(cx).value().is_empty())
            .unwrap()
    );
    view.update_in(cx, |v, w, cx| v.open_mutation(image_id, true, w, cx))
        .unwrap();
    stage(&output, "delete", cx).await;
    let form = view
        .update_in(cx, |v, _, _| {
            v.mutation.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    cx.update(|w, cx| form.update(cx, |f, cx| f.submit(w, cx)))
        .unwrap();
    wait(&view, cx, move |v, _| {
        v.store.get(image_id).is_some_and(|m| m.deleted())
    })
    .await;
    stage(&output, "final", cx).await;
    std::fs::write(&output,serde_json::to_vec_pretty(&json!({"passed":true,"checks":["Markdown native render","official server edit and delete","stale edit rejection and repeated Escape dialogs","native text clipboard paste","room identity completion and explicit mention IDs","native PNG and JPEG clipboard offers","preview without automatic send","two official uploads","explicit same-server bounded image loading","private opt-in save and ordered reset"]})).unwrap()).unwrap();
    cx.update(|_, cx| cx.quit()).unwrap();
}
