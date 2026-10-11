use apron_gpui::transport::{Client, Event};
use serde_json::{Value, json};
use std::time::Duration;

pub async fn connection(server: &str) -> (Client, async_channel::Receiver<Event>) {
    let (tx, rx) = async_channel::bounded(256);
    (Client::connect(server, Some(tx)).await.unwrap(), rx)
}
/// A same-socket RPC response is a barrier after queued server notifications.
pub async fn drain(client: &mut Client, rx: &async_channel::Receiver<Event>) -> Vec<Value> {
    client.rpc("me", json!({})).await.unwrap();
    let mut frames = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let Event::Notice(frame) = event {
            frames.push(frame);
        }
    }
    frames
}
/// Status coalescing may intentionally delay a notification by two seconds.
pub async fn expect(
    client: &mut Client,
    rx: &async_channel::Receiver<Event>,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(frame) = drain(client, rx).await.into_iter().find(&predicate) {
                return frame;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("Expected synthetic peer notification was not received")
}

/// Synthetic account via the supported development-only sender; no SMTP/passkeys.
pub async fn account(
    address: &str,
) -> (
    Client,
    async_channel::Receiver<Event>,
    apron_gpui::auth::Secret,
    Value,
) {
    use apron_gpui::{
        auth::Secret,
        email::{Request, Session},
    };
    const ROLES: &str = "ws://127.0.0.1:18767/ws";
    assert!(matches!(
        address,
        "moderator@fixture.invalid" | "presence@fixture.invalid"
    ));
    let path = std::env::var("APRON_SYNTHETIC_ROLE_LOG").expect("Use the isolated loopback runner");
    let session = Session::start(ROLES.into());
    session
        .commands
        .send(Request::Propose(address.into()))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), session.results.recv())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .is_none()
    );
    // The supported development sender writes only to a private temporary owned log.
    // Never print, persist in evidence, or send these ephemeral codes elsewhere.
    let code = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            assert!(std::fs::metadata(&path).unwrap().len() < 1_048_576);
            let log = std::fs::read_to_string(&path).unwrap();
            if let Some(code) = log
                .lines()
                .filter(|l| l.contains(&format!("to={address}")))
                .rev()
                .find_map(|l| {
                    l.split_whitespace()
                        .find_map(|part| part.strip_prefix("code="))
                })
            {
                break Secret::new(code.trim_matches('"').to_owned()).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Synthetic development code delivery");
    session.commands.send(Request::Approve(code)).await.unwrap();
    let token = tokio::time::timeout(Duration::from_secs(5), session.results.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .expect("Synthetic account token");
    let (mut moderator, rx) = connection(ROLES).await;
    let identity = moderator.authenticate(Some(&token)).await.unwrap();
    let token = Secret::new(
        identity["token"]
            .as_str()
            .expect("Rotated synthetic token")
            .to_owned(),
    )
    .unwrap();
    (moderator, rx, token, identity)
}
