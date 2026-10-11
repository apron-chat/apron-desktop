//! Explicit opt-in native checks against the isolated official loopback server.
use super::*;
use crate::smoke::{key, settle, wait};
use gpui_kit::InputEvent as _;
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
async fn presence_check(view: &WeakEntity<Conversation>, cx: &mut AsyncWindowContext) {
    eprintln!("presence: connecting peer");
    let peer = apron_gpui::transport::Bridge::start("ws://127.0.0.1:18766/ws".into()).unwrap();
    let uid = loop {
        if let apron_gpui::transport::Event::Ready { you, .. } = peer.events.recv().await.unwrap() {
            break you;
        }
    };
    peer.commands
        .try_send(Command::Manage {
            operation: 9001,
            method: "me".into(),
            payload: json!({"status":"dnd"}),
        })
        .unwrap();
    eprintln!("presence: peer DND notice");
    wait(view, cx, |v, _| {
        v.users.get(&uid).is_some_and(|u| u["status"] == "dnd")
    })
    .await;
    // Reconnect must reconstruct presence from room_list, without a fresh notice.
    eprintln!("presence: reconnect snapshot");
    let generation = view.update_in(cx, |v, _, _| v.generation).unwrap();
    view.update_in(cx, |v, _, cx| {
        v.command(Command::Reconnect, cx);
    })
    .unwrap();
    wait(view, cx, |v, _| {
        v.ready && !v.loading && v.generation > generation
    })
    .await;
    assert!(
        view.update_in(cx, |v, _, _| v
            .users
            .get(&uid)
            .is_some_and(|u| u["status"] == "dnd"))
            .unwrap(),
        "room_list must retain peer presence"
    );
    eprintln!("presence: typing expiry");
    let room = view.update_in(cx, |v, _, _| v.room.clone()).unwrap();
    peer.commands
        .try_send(Command::Typing {
            room: room.clone(),
            seconds: 1,
        })
        .unwrap();
    wait(view, cx, |v, _| v.typing_label().is_some()).await;
    wait(view, cx, |v, _| v.typing_label().is_none()).await;
    peer.commands
        .try_send(Command::Typing {
            room: room.clone(),
            seconds: 8,
        })
        .unwrap();
    wait(view, cx, |v, _| v.typing_label().is_some()).await;
    peer.commands
        .try_send(Command::Typing {
            room: room.clone(),
            seconds: 0,
        })
        .unwrap();
    wait(view, cx, |v, _| v.typing_label().is_none()).await;
    eprintln!("presence: mute on/off");
    for scope in [&room, ""] {
        view.update_in(cx, |v, _, cx| {
            v.set_status(
                if scope.is_empty() {
                    json!({"mute":true})
                } else {
                    json!({"room_id":scope,"mute":true})
                },
                "status",
                cx,
            )
        })
        .unwrap();
        wait(view, cx, |v, _| v.muted(&room)).await;
        view.update_in(cx, |v, _, cx| {
            v.set_status(
                if scope.is_empty() {
                    json!({"mute":false})
                } else {
                    json!({"room_id":scope,"mute":false})
                },
                "status",
                cx,
            )
        })
        .unwrap();
        wait(view, cx, |v, _| !v.muted(&room)).await;
    }
    eprintln!("presence: timed mute");
    view.update_in(cx, |v, _, cx| {
        v.set_status(json!({"room_id":room,"mute":1}), "status", cx)
    })
    .unwrap();
    wait(view, cx, |v, _| v.muted(&room)).await;
    wait(view, cx, |v, _| !v.muted(&room)).await;
}
async fn run(view: WeakEntity<Conversation>, output: String, cx: &mut AsyncWindowContext) {
    wait(&view, cx, |v, _| v.ready && !v.loading && !v.local_loading).await;
    presence_check(&view, cx).await;
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
    view.update_in(cx, |v, _, cx| v.react(id, "👍", cx))
        .unwrap();
    wait(&view, cx, |v, _| {
        v.reacting.is_none() && v.reactions.own(id, &v.you).contains("👍")
    })
    .await;
    stage(&output, "official-reaction", cx).await;
    view.update_in(cx, |v, _, cx| v.react(id, "👍", cx))
        .unwrap();
    wait(&view, cx, |v, _| {
        v.reacting.is_none() && v.reactions.own(id, &v.you).is_empty()
    })
    .await;

    // Search and apply a non-preset Unicode sequence through the same safe actor.
    view.update_in(cx, |v, w, cx| v.emoji_dialog(id, w, cx))
        .unwrap();
    let picker = view
        .update_in(cx, |v, _, _| {
            v.emoji_form.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    cx.update(|w, cx| {
        picker.update(cx, |f, cx| {
            f.input.update(cx, |s, cx| s.set_value("👩‍💻", w, cx));
            f.choose("👩‍💻", cx);
        })
    })
    .unwrap();
    stage(&output, "emoji-picker", cx).await;
    cx.update(|w, cx| picker.update(cx, |f, cx| f.submit(w, cx)))
        .unwrap();
    wait(&view, cx, |v, _| {
        v.reacting.is_none() && v.reactions.own(id, &v.you).contains("👩‍💻")
    })
    .await;
    settle(cx).await;
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
    cx.update(|w, cx| crate::smoke::key("escape", w, cx))
        .unwrap();
    settle(cx).await;
    for _ in 0..2 {
        view.update_in(cx, |v, w, cx| v.open_mutation(id, false, w, cx))
            .unwrap();
        settle(cx).await;
        cx.update(|w, cx| crate::smoke::key("escape", w, cx))
            .unwrap();
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
                avatar: false,
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
    // Full-size viewer shares the actor's bounded transfer lane; opening never fetches.
    let (viewer_url, viewer_key) = view
        .update_in(cx, |v, _, _| {
            let m = v.store.get(image_id).unwrap();
            let u = m.raw["body"]["embeds"][0]["url"]
                .as_str()
                .unwrap()
                .to_string();
            (u.clone(), Conversation::image_key(image_id, 0, &u))
        })
        .unwrap();
    wait(&view, cx, |v, _| !v.transfer_busy).await;
    view.update_in(cx, |v, w, cx| {
        v.open_viewer(viewer_url.clone(), Some(viewer_key.clone()), None, w, cx)
    })
    .unwrap();
    let viewer = view
        .update_in(cx, |v, _, _| v.viewer.as_ref().unwrap().upgrade().unwrap())
        .unwrap();
    cx.update(|_, cx| {
        viewer.update(cx, |f, cx| {
            assert!(!f.loading && f.raster.is_none());
            f.load(cx);
        })
    })
    .unwrap();
    wait(&view, cx, |v, cx| {
        v.viewer
            .as_ref()
            .and_then(|f| f.upgrade())
            .is_some_and(|f| f.read(cx).raster.is_some())
    })
    .await;
    stage(&output, "viewer-fit", cx).await;
    let avatar_raster = cx
        .update(|_, cx| {
            viewer.update(cx, |f, cx| {
                f.zoom_by(8., cx);
                assert_eq!(f.zoom, Some(4.));
                apron_gpui::images::normalize(&f.raster.as_ref().unwrap().png).unwrap()
            })
        })
        .unwrap();
    stage(&output, "viewer-zoom", cx).await;
    cx.update(|w, cx| crate::smoke::key("escape", w, cx))
        .unwrap();
    wait(&view, cx, |v, _| !v.transfer_busy).await;
    // Closing releases decoded bytes and rejects a cancelled load's late completion.
    cx.update(|_, cx| assert!(viewer.read(cx).raster.is_none()))
        .unwrap();
    view.update_in(cx, |v, w, cx| {
        v.open_viewer(viewer_url.clone(), Some(viewer_key.clone()), None, w, cx)
    })
    .unwrap();
    let cancelled = view
        .update_in(cx, |v, _, _| v.viewer.as_ref().unwrap().upgrade().unwrap())
        .unwrap();
    cx.update(|w, cx| {
        cancelled.update(cx, |f, cx| {
            f.load(cx);
            f.dismiss(w, cx);
        });
    })
    .unwrap();
    wait(&view, cx, |v, _| !v.transfer_busy).await;
    cx.update(|_, cx| assert!(cancelled.read(cx).raster.is_none()))
        .unwrap();
    // Confirming an avatar upload is distinct from merely choosing/preparing it.
    view.update_in(cx, |v, w, cx| v.avatar_upload_dialog(avatar_raster, w, cx))
        .unwrap();
    stage(&output, "avatar-confirm", cx).await;
    let upload = view
        .update_in(cx, |v, _, _| {
            v.avatar_upload.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    cx.update(|_, cx| {
        upload.update(cx, |f, cx| {
            assert!(f.pending.is_none());
            f.submit(cx);
        })
    })
    .unwrap();
    wait(&view, cx, |v, _| {
        !v.transfer_busy && v.avatar_source(&v.you).is_some()
    })
    .await;
    view.update_in(cx, |v, w, cx| {
        assert!(v.avatar_cache.is_empty());
        v.avatar_dialog(v.you.clone(), w, cx);
    })
    .unwrap();
    let avatar = view
        .update_in(cx, |v, _, _| v.viewer.as_ref().unwrap().upgrade().unwrap())
        .unwrap();
    cx.update(|_, cx| avatar.update(cx, |f, cx| f.load(cx)))
        .unwrap();
    wait(&view, cx, |v, cx| {
        v.viewer
            .as_ref()
            .and_then(|f| f.upgrade())
            .is_some_and(|f| f.read(cx).raster.is_some())
    })
    .await;
    stage(&output, "avatar-preview", cx).await;
    cx.update(|w, cx| {
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(w), cx);
        crate::design::apply(cx);
    })
    .unwrap();
    stage(&output, "avatar-dark", cx).await;
    cx.update(|w, cx| {
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Light, Some(w), cx);
        crate::design::apply(cx);
    })
    .unwrap();
    cx.update(|w, cx| {
        avatar.update(cx, |f, cx| {
            f.use_avatar(cx);
            f.dismiss(w, cx);
        });
    })
    .unwrap();
    assert!(
        view.update_in(cx, |v, _, _| v.avatar_cache.len() == 1)
            .unwrap()
    );
    let (avatar_user, original_source) = view
        .update_in(cx, |v, _, _| {
            let original = v.users[&v.you]["avatar"].clone();
            v.users.get_mut(&v.you).unwrap()["avatar"] =
                json!("https://foreign.invalid/blocked.png");
            (v.you.clone(), original)
        })
        .unwrap();
    cx.update(|_, cx| assert!(Conversation::avatar_image(&avatar_user, &view, cx).is_none()))
        .unwrap();
    view.update_in(cx, |v, _, _| {
        v.users.get_mut(&avatar_user).unwrap()["avatar"] = original_source
    })
    .unwrap();
    // Exercise confirmation and a real server denial; no request on initial Review.
    view.update_in(cx, |v, w, cx| {
        v.manage_dialog(apron_gpui::workflows::Kind::Kick, w, cx)
    })
    .unwrap();
    let moderation = view
        .update_in(cx, |v, _, _| {
            v.manage_form.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    let me = view.update_in(cx, |v, _, _| v.you.clone()).unwrap();
    cx.update(|w, cx| {
        moderation.update(cx, |f, cx| {
            f.user.update(cx, |s, cx| s.set_value(me, w, cx));
            f.description
                .update(cx, |s, cx| s.set_value("Synthetic review reason", w, cx));
            f.submit(w, cx);
            assert!(f.confirmed);
        })
    })
    .unwrap();
    assert!(
        view.update_in(cx, |v, _, _| v.manage_pending.is_none())
            .unwrap()
    );
    stage(&output, "moderation-confirm", cx).await;
    cx.update(|w, cx| moderation.update(cx, |f, cx| f.submit(w, cx)))
        .unwrap();
    wait(&view, cx, |v, cx| {
        v.manage_form
            .as_ref()
            .and_then(|f| f.upgrade())
            .is_some_and(|f| !f.read(cx).error.is_empty())
    })
    .await;
    stage(&output, "moderation-denied", cx).await;
    cx.update(|w, cx| w.close_dialog(cx)).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let copied = temporary.path().join("copied.txt");
    let dropped = temporary.path().join("dropped.txt");
    std::fs::write(&copied, b"Synthetic clipboard file").unwrap();
    std::fs::write(&dropped, b"Synthetic dropped file").unwrap();
    clipboard(
        format!("{}\r\n", url::Url::from_file_path(&copied).unwrap()).into_bytes(),
        "text/uri-list",
        cx,
    )
    .await;
    view.update_in(cx, |v, w, cx| {
        v.composer.update(cx, |s, cx| s.focus(w, cx));
    })
    .unwrap();
    cx.update(|w, cx| crate::smoke::key("ctrl-v", w, cx))
        .unwrap();
    wait(&view, cx, |v, _| {
        v.clipboard_task.is_none() && v.files.len() == 1
    })
    .await;
    // Inject platform file-drop events through GPUI's native dispatch, not the staging method.
    // Compositor-to-client data-offer interoperability still needs a real file manager.
    let position = cx
        .update(|w, _| {
            point(
                w.viewport_size().width / 2.,
                w.viewport_size().height - px(64.),
            )
        })
        .unwrap();
    cx.update(|w, cx| {
        w.dispatch_event(
            FileDropEvent::Entered {
                position,
                paths: ExternalPaths(vec![dropped.clone()].into()),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    settle(cx).await;
    stage(&output, "file-drop-hover", cx).await;
    cx.update(|w, cx| {
        w.dispatch_event(FileDropEvent::Submit { position }.to_platform_input(), cx);
    })
    .unwrap();
    wait(&view, cx, |v, _| {
        v.clipboard_task.is_none() && v.files.len() == 2
    })
    .await;
    assert!(view.update_in(cx, |v, _, _| !v.pending).unwrap());
    stage(&output, "file-attachments", cx).await;
    view.update_in(cx, |v, w, cx| v.rename_attachment(1, w, cx))
        .unwrap();
    let rename = view
        .update_in(cx, |v, _, _| {
            v.title_form.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    cx.update(|w, cx| {
        rename.update(cx, |f, cx| {
            f.input.update(cx, |s, cx| s.set_value("", w, cx));
            f.submit(w, cx);
            assert!(!f.error.is_empty());
        })
    })
    .unwrap();
    cx.update(|w, cx| {
        rename.update(cx, |f, cx| {
            f.input
                .update(cx, |s, cx| s.set_value("Shared design notes.txt", w, cx))
        })
    })
    .unwrap();
    stage(&output, "rename-attachment", cx).await;
    cx.update(|w, cx| rename.update(cx, |f, cx| f.submit(w, cx)))
        .unwrap();
    assert!(dropped.exists());

    // Cancellation must not append even when the bounded read has already started.
    view.update_in(cx, |v, _, cx| {
        v.stage_paths(vec![copied.clone()], cx);
        v.clipboard_serial += 1;
        v.clipboard_task = None;
    })
    .unwrap();
    settle(cx).await;
    assert_eq!(view.update_in(cx, |v, _, _| v.files.len()).unwrap(), 2);
    view.update_in(cx, |v, _, cx| {
        v.stage_paths(vec![temporary.path().to_owned()], cx)
    })
    .unwrap();
    wait(&view, cx, |v, _| {
        v.clipboard_task.is_none() && !v.error.is_empty()
    })
    .await;
    assert_eq!(view.update_in(cx, |v, _, _| v.files.len()).unwrap(), 2);
    stage(&output, "attachment-error", cx).await;
    view.update_in(cx, |v, w, cx| v.send(w, cx)).unwrap();
    wait(&view, cx, |v, _| !v.pending && v.files.is_empty()).await;
    assert!(
        view.update_in(cx, |v, _, _| v.rows.iter().any(|m| m.raw["body"]["embeds"]
            .as_array()
            .is_some_and(|es| es
                .iter()
                .any(|e| e["title"] == "Shared design notes.txt"))))
            .unwrap()
    );
    let sent = view
        .update_in(cx, |v, _, _| {
            v.rows
                .iter()
                .find(|m| {
                    m.raw["body"]["embeds"].as_array().is_some_and(|es| {
                        es.iter().any(|e| e["title"] == "Shared design notes.txt")
                    })
                })
                .unwrap()
                .id
        })
        .unwrap();
    wait(&view, cx, move |v, _| {
        !v.transfer_busy
            && v.store.get(sent).is_some_and(|m| {
                m.raw["body"]["embeds"].as_array().is_some_and(|es| {
                    es.len() == 2
                        && es
                            .iter()
                            .all(|e| e["url"].as_str().is_some_and(|u| !u.is_empty()))
                })
            })
    })
    .await;
    let original = view
        .update_in(cx, |v, _, _| v.store.get(sent).unwrap().raw.clone())
        .unwrap();
    view.update_in(cx, |v, w, cx| v.open_mutation(sent, false, w, cx))
        .unwrap();
    let editor = view
        .update_in(cx, |v, _, _| {
            v.mutation.as_ref().unwrap().upgrade().unwrap()
        })
        .unwrap();
    cx.update(|w, cx| {
        editor.update(cx, |f, cx| {
            f.titles[1]
                .1
                .update(cx, |s, cx| s.set_value("Revised shared notes.txt", w, cx))
        })
    })
    .unwrap();
    stage(&output, "edit-attachment-title", cx).await;
    cx.update(|w, cx| editor.update(cx, |f, cx| f.submit(w, cx)))
        .unwrap();
    wait(&view, cx, move |v, _| {
        v.mutation_pending.is_none()
            && v.store
                .get(sent)
                .is_some_and(|m| m.raw["body"]["embeds"][1]["title"] == "Revised shared notes.txt")
    })
    .await;
    view.update_in(cx, |v, _, _| {
        let body = &v.store.get(sent).unwrap().raw["body"];
        assert_eq!(body["text"], original["body"]["text"]);
        for i in 0..2 {
            assert_eq!(
                body["embeds"][i]["url"],
                original["body"]["embeds"][i]["url"]
            );
            assert_eq!(
                body["embeds"][i]["embed_id"],
                original["body"]["embeds"][i]["embed_id"]
            );
        }
    })
    .unwrap();
    // Reload preferences without restoring private draft contents when storage is off.
    view.update_in(cx, |v, w, cx| {
        v.change_preference(local::Preference::Theme, w, cx);
        v.change_preference(local::Preference::Notifications, w, cx);
        v.change_preference(local::Preference::Typing, w, cx);
        v.change_preference(local::Preference::Idle, w, cx);
    })
    .unwrap();
    wait(&view, cx, |v, _| v.local_status == "Saved locally").await;
    view.update_in(cx, |v, w, cx| {
        v.profile = None;
        v.notifications = false;
        v.preferences = Default::default();
        v.load_profile(w, cx);
    })
    .unwrap();
    wait(&view, cx, |v, _| !v.local_loading).await;
    assert!(
        view.update_in(cx, |v, _, _| v.notifications
            && v.preferences.dark
            && v.preferences.suppress_typing
            && v.preferences.report_idle
            && !v.local_enabled)
            .unwrap()
    );
    view.update_in(cx, |v, w, cx| {
        v.last_input = std::time::Instant::now() - Duration::from_secs(301);
        v.update_idle(w, cx);
        assert_eq!(v.idle_reported, Some(true));
        v.record_input(w, cx);
        assert_eq!(v.idle_reported, Some(false));
    })
    .unwrap();
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
    std::fs::write(&output,serde_json::to_vec_pretty(&json!({"passed":true,"checks":["multi-client presence snapshot after reconnect, typing expiry and stop, scoped/global/timed mute", "Markdown native render","official reaction add/remove through transport actor","official server edit and delete","stale edit rejection and repeated Escape dialogs","native text clipboard paste","room identity completion and explicit mention IDs","native PNG and JPEG clipboard offers","preview without automatic send","two official uploads","explicit same-server bounded image loading","private opt-in save and ordered reset","emoji picker Unicode sequence through reconciled reaction actor","real URI-list clipboard files","platform-dispatched file drop and explicit upload","attachment cancellation and directory error preserve prior files","durable appearance/notification/privacy reload","idle threshold and return to attended","pre-send shared title validation and rename","post-send embed title change preserves URL ID and caption","explicit full-size viewer fit zoom Escape cancellation and decoded-data release","confirmed avatar upload and user-controlled cached display","moderation review sends nothing and server denial preserves form"]})).unwrap()).unwrap();
    cx.update(|_, cx| cx.quit()).unwrap();
}
