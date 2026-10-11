//! Explicitly opt in: these tests use only dedicated synthetic loopback servers.
use apron_gpui::{
    media,
    model::{number, text},
    transport::{Bridge, Client, Command, Event},
};
use serde_json::json;
use std::time::Duration;
const OFFICIAL: &str = "ws://127.0.0.1:18766/ws";
#[tokio::test]
#[ignore = "requires the dedicated official loopback Apron server"]
async fn official_history_reply_upload_and_reconnect() {
    let mut c = Client::connect(OFFICIAL, None).await.unwrap();
    let auth = c.guest().await.unwrap();
    assert!(auth["you"]["user_id"].is_string());
    let rooms = c.rpc("room_list", json!({"filter":"all"})).await.unwrap();
    assert!(
        rooms["joined"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["room_id"] == "general")
    );
    let room = c
        .rpc("room_set", json!({"title":"Synthetic GPUI interop"}))
        .await
        .unwrap();
    let rid = text(&room["room_id"]);
    let mut ids = Vec::new();
    for i in 0..65 {
        let m = c
            .rpc(
                "message",
                json!({"room_id":rid,"body":{"text":format!("Synthetic row {i}")}}),
            )
            .await
            .unwrap();
        ids.push(m["message_id"].clone());
    }
    let latest = c.history(rid, None).await.unwrap();
    assert_eq!(latest["messages"].as_array().unwrap().len(), 50);
    assert_eq!(latest["more"], true);
    let before = number(&latest["first_log_id"]).unwrap() - 1;
    let older = c.history(rid, Some(before)).await.unwrap();
    assert!(older["messages"].as_array().unwrap().len() >= 15);
    let posted=c.rpc("message",json!({"room_id":rid,"reply_to":{"message_id":ids[0]},"body":{"text":"Two synthetic files","embeds":[{"kind":"upload","title":"one.txt"},{"kind":"upload","title":"two.txt"}]}})).await.unwrap();
    let urls = posted["embeds"].as_array().unwrap();
    assert_eq!(urls.len(), 2);
    for (i, e) in urls.iter().enumerate() {
        media::put(
            text(&e["write_url"]),
            OFFICIAL,
            format!("Synthetic bytes {i}").into_bytes(),
        )
        .await
        .unwrap();
    }
    let page = c.history(rid, None).await.unwrap();
    let msg = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|m| m["message_id"] == posted["message_id"])
        .unwrap();
    assert_eq!(msg["reply_to"]["message_id"], ids[0]);
    for (i, e) in msg["body"]["embeds"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            media::get(text(&e["url"]), OFFICIAL).await.unwrap(),
            format!("Synthetic bytes {i}").into_bytes()
        );
    }
    assert!(
        media::put(text(&urls[0]["write_url"]), OFFICIAL, b"retry".to_vec())
            .await
            .is_err()
    );
    drop(c);
    let mut resumed = Client::connect(OFFICIAL, None).await.unwrap();
    resumed.guest().await.unwrap();
    assert!(!resumed.history("general", None).await.unwrap()["messages"].is_null());
}
#[tokio::test]
#[ignore = "requires the synthetic fixture on port 18765"]
async fn actor_reports_interruption_without_resending() {
    let bridge = Bridge::start("ws://127.0.0.1:18765/ws".into()).unwrap();
    async fn next(b: &Bridge) -> Event {
        tokio::time::timeout(Duration::from_secs(12), b.events.recv())
            .await
            .unwrap()
            .unwrap()
    }
    loop {
        if matches!(next(&bridge).await, Event::Ready { .. }) {
            break;
        }
    }
    bridge
        .commands
        .send(Command::History {
            generation: 11,
            room: "general".into(),
            before: None,
        })
        .await
        .unwrap();
    loop {
        if let Event::History { generation, .. } = next(&bridge).await {
            assert_eq!(generation, 11);
            break;
        }
    }
    bridge
        .commands
        .send(Command::Send {
            generation: 12,
            room: "general".into(),
            text: "/test-disconnect".into(),
            mentions: vec![],
            reply: None,
            files: vec![],
        })
        .await
        .unwrap();
    let mut failed = false;
    loop {
        match next(&bridge).await {
            Event::Failure {
                generation: Some(12),
                ..
            } => failed = true,
            Event::Ready { .. } => {
                assert!(failed);
                break;
            }
            Event::Sent { generation: 12, .. } => panic!("Disconnected send claimed success"),
            _ => {}
        }
    }
    bridge
        .commands
        .send(Command::History {
            generation: 13,
            room: "general".into(),
            before: None,
        })
        .await
        .unwrap();
    loop {
        if let Event::History {
            generation, page, ..
        } = next(&bridge).await
        {
            assert_eq!(generation, 13);
            assert!(
                !page["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["body"]["text"] == "/test-disconnect")
            );
            break;
        }
    }
}

#[tokio::test]
#[ignore = "requires the dedicated official loopback Apron server"]
async fn joining_uses_membership_rpc_and_preserves_room_fields() {
    let mut owner = Client::connect(OFFICIAL, None).await.unwrap();
    owner.guest().await.unwrap();
    let created = owner
        .rpc(
            "room_set",
            json!({"title":"Synthetic join preserves title","description":"Unchanged by a join"}),
        )
        .await
        .unwrap();
    let room = text(&created["room_id"]).to_owned();
    let bridge = Bridge::start(OFFICIAL.into()).unwrap();
    async fn next(bridge: &Bridge) -> Event {
        tokio::time::timeout(Duration::from_secs(8), bridge.events.recv())
            .await
            .unwrap()
            .unwrap()
    }
    loop {
        if matches!(next(&bridge).await, Event::Ready { .. }) {
            break;
        }
    }
    bridge
        .commands
        .send(Command::Join { room: room.clone() })
        .await
        .unwrap();
    loop {
        match next(&bridge).await {
            Event::Notice(frame) if frame["method"] == "room_update" => {
                if let Some(joined) = frame["params"]["joined"].as_array()
                    && let Some(record) = joined.iter().find(|r| r["room_id"] == room)
                {
                    assert_eq!(record["title"], "Synthetic join preserves title");
                    assert_eq!(record["description"], "Unchanged by a join");
                    break;
                }
            }
            Event::Failure { message, .. } => panic!("Join failed: {message}"),
            _ => {}
        }
    }
    let list = owner
        .rpc("room_list", json!({"filter":"all"}))
        .await
        .unwrap();
    let record = list["joined"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["room_id"] == room)
        .unwrap();
    assert_eq!(record["title"], "Synthetic join preserves title");
    assert_eq!(record["description"], "Unchanged by a join");
}

#[tokio::test]
#[ignore = "requires the dedicated official loopback Apron server"]
async fn official_edit_delete_preserves_metadata_and_enforces_author() {
    let mut c = Client::connect(OFFICIAL, None).await.unwrap();
    c.guest().await.unwrap();
    let room = c
        .rpc(
            "room_set",
            json!({"title":"Synthetic edit and image checks"}),
        )
        .await
        .unwrap();
    let rid = text(&room["room_id"]);
    let first = c
        .rpc("message", json!({"room_id":rid,"body":{"text":"Parent"}}))
        .await
        .unwrap();
    let posted=c.rpc("message",json!({"room_id":rid,"reply_to":{"message_id":first["message_id"]},"body":{"format":"markdown","text":"**Before**","mentions":[],"embeds":[{"kind":"upload","title":"test.png"}]}})).await.unwrap();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(32, 20)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    media::put(
        text(&posted["embeds"][0]["write_url"]),
        OFFICIAL,
        bytes.into_inner(),
    )
    .await
    .unwrap();
    let page = c.history(rid, None).await.unwrap();
    let original = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|m| m["message_id"] == posted["message_id"])
        .unwrap()
        .clone();
    let mut other = Client::connect(OFFICIAL, None).await.unwrap();
    other.guest().await.unwrap();
    other
        .rpc("room_join", json!({"room_id":rid}))
        .await
        .unwrap();
    assert!(
        other
            .rpc(
                "message",
                apron_gpui::edit::payload(&original, Some("Unauthorized")).unwrap()
            )
            .await
            .is_err()
    );
    c.rpc(
        "message",
        apron_gpui::edit::payload(&original, Some("**After**")).unwrap(),
    )
    .await
    .unwrap();
    let page = c.history(rid, None).await.unwrap();
    let edited = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|m| m["message_id"] == posted["message_id"])
        .unwrap();
    assert_eq!(edited["body"]["text"], "**After**");
    assert_eq!(
        edited["body"]["embeds"][0]["embed_id"],
        original["body"]["embeds"][0]["embed_id"]
    );
    assert_eq!(
        edited["body"]["embeds"][0]["title"],
        original["body"]["embeds"][0]["title"]
    );
    assert_eq!(edited["reply_to"]["message_id"], first["message_id"]);
    let raw = media::get(text(&edited["body"]["embeds"][0]["url"]), OFFICIAL)
        .await
        .unwrap();
    let raster = apron_gpui::images::normalize(&raw).unwrap();
    assert_eq!((raster.width, raster.height), (32, 20));
    c.rpc("message", apron_gpui::edit::payload(edited, None).unwrap())
        .await
        .unwrap();
    let page = c.history(rid, None).await.unwrap();
    assert!(
        page["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["message_id"] == posted["message_id"] && m["deleted"] == true)
    );
}

#[tokio::test]
#[ignore = "requires official synthetic loopback server"]
async fn official_reactions_replace_remove_and_reconcile_history() {
    use apron_gpui::reactions::Reactions;
    let mut c = Client::connect(OFFICIAL, None).await.unwrap();
    let auth = c.guest().await.unwrap();
    let user = text(&auth["you"]["user_id"]);
    let room = c
        .rpc(
            "room_set",
            json!({"title":"Synthetic reaction replacement"}),
        )
        .await
        .unwrap();
    let rid = text(&room["room_id"]);
    let m = c
        .rpc(
            "message",
            json!({"room_id":rid,"body":{"text":"React here"}}),
        )
        .await
        .unwrap();
    let mid = m["message_id"].clone();
    c.rpc("reactions", json!({"message_id":mid,"emojis":["👍","❤️"]}))
        .await
        .unwrap();
    let before = c.history(rid, None).await.unwrap();
    let mut state = Reactions::default();
    for r in before["reactions"].as_array().unwrap() {
        state.apply(r);
    }
    assert_eq!(state.totals(number(&mid).unwrap()).len(), 2);
    let boundary = number(&before["last_log_id"]).unwrap();
    c.rpc("reactions", json!({"message_id":mid,"emojis":["❤️"]}))
        .await
        .unwrap();
    let page = c.history(rid, Some(boundary)).await.unwrap();
    for r in page["reactions"].as_array().unwrap() {
        state.apply(r);
    }
    assert_eq!(
        state.own(number(&mid).unwrap(), user),
        ["❤️".to_string()].into_iter().collect()
    );
    c.rpc("reactions", json!({"message_id":mid,"emojis":[]}))
        .await
        .unwrap();
    let page = c.history(rid, None).await.unwrap();
    for r in page["reactions"].as_array().unwrap() {
        state.apply(r);
    }
    assert!(state.totals(number(&mid).unwrap()).is_empty());
}

#[tokio::test]
#[ignore = "requires the dedicated official loopback Apron server"]
async fn official_room_forms_create_thread_edit_and_leave() {
    use apron_gpui::workflows::Kind;
    let mut c = Client::connect(OFFICIAL, None).await.unwrap();
    c.guest().await.unwrap();
    let (method, params) = Kind::Create
        .request(&json!({}), "Synthetic forms", "Original description", "")
        .unwrap();
    let created = c.rpc(method, params).await.unwrap();
    let room = json!({"room_id":created["room_id"]});
    let (method, params) = Kind::Thread
        .request(&room, "Synthetic thread", "Thread summary", "")
        .unwrap();
    let thread = c.rpc(method, params).await.unwrap();
    let (method, params) = Kind::Settings
        .request(&room, "Renamed synthetic room", "Updated description", "")
        .unwrap();
    c.rpc(method, params).await.unwrap();
    let list = c
        .rpc("room_list", json!({"filter":"joined","members":true}))
        .await
        .unwrap();
    assert!(
        list["joined"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["room_id"] == created["room_id"] && r["title"] == "Renamed synthetic room")
    );
    assert!(list["joined"].as_array().unwrap().iter().any(|r|r["room_id"]==thread["room_id"] && r["parent_room_id"]==created["room_id"]));
    let (method, params) = Kind::Leave.request(&room, "", "", "").unwrap();
    c.rpc(method, params).await.unwrap();
    let list = c
        .rpc("room_list", json!({"filter":"joined"}))
        .await
        .unwrap();
    assert!(
        !list["joined"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["room_id"] == created["room_id"])
    );
}

#[tokio::test]
#[ignore = "requires the dedicated official loopback Apron server"]
async fn official_membership_forms_add_and_remove_another_user() {
    use apron_gpui::workflows::Kind;
    let mut owner = Client::connect(OFFICIAL, None).await.unwrap();
    owner.guest().await.unwrap();
    let mut peer = Client::connect(OFFICIAL, None).await.unwrap();
    let peer_auth = peer.guest().await.unwrap();
    let peer_id = text(&peer_auth["you"]["user_id"]);
    let created = owner
        .rpc(
            "room_set",
            json!({"title":"Synthetic membership forms", "private":true}),
        )
        .await
        .unwrap();
    let room = json!({"room_id":created["room_id"]});
    let (method, params) = Kind::AddMember.request(&room, "", "", peer_id).unwrap();
    owner.rpc(method, params).await.unwrap();
    let list = peer
        .rpc("room_list", json!({"filter":"joined"}))
        .await
        .unwrap();
    assert!(
        list["joined"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["room_id"] == room["room_id"])
    );
    let (method, params) = Kind::RemoveMember.request(&room, "", "", peer_id).unwrap();
    owner.rpc(method, params).await.unwrap();
    let list = peer
        .rpc("room_list", json!({"filter":"joined"}))
        .await
        .unwrap();
    assert!(
        !list["joined"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["room_id"] == room["room_id"])
    );
    assert!(
        peer.rpc(
            "message",
            json!({"room_id":room["room_id"],"body":{"text":"Synthetic forbidden send"}})
        )
        .await
        .is_err()
    );
}
