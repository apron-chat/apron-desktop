//! Ephemeral loopback servers and synthetic tokens only.
use apron_gpui::{
    auth::Secret,
    transport::{Bridge, Command, Event},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn next(bridge: &Bridge) -> Event {
    tokio::time::timeout(Duration::from_secs(8), bridge.events.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn guest_identity_resumes_with_rotated_token() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for connection in 0..3 {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(socket).await.unwrap();
            ws.send(Message::Text(json!({"method":"server","params":{"apron":8,"auth":["guest","token"],"capabilities":[],"ping":20}}).to_string().into())).await.unwrap();
            while let Some(Ok(Message::Text(raw))) = ws.next().await {
                let req: Value = serde_json::from_str(&raw).unwrap();
                let result = match req["method"].as_str().unwrap() {
                    "auth" => {
                        if connection == 0 {
                            assert_eq!(req["params"]["scheme"], "guest");
                        } else {
                            assert_eq!(req["params"]["scheme"], "token");
                            assert_eq!(
                                req["params"]["token"],
                                format!("synthetic-rotation-{connection}")
                            );
                        }
                        json!({"you":{"user_id":"synthetic-stable-user"},"token":format!("synthetic-rotation-{}",connection+1)})
                    }
                    "room_list" => json!({"joined":[],"not_joined":[]}),
                    _ => panic!("Unexpected request"),
                };
                ws.send(Message::Text(
                    json!({"id":req["id"],"result":result}).to_string().into(),
                ))
                .await
                .unwrap();
            }
        }
    });
    let bridge = Bridge::start(format!("ws://{address}/ws")).unwrap();
    for connection in 0..3 {
        loop {
            if let Event::Ready {
                you, authenticated, ..
            } = next(&bridge).await
            {
                assert_eq!(you, "synthetic-stable-user");
                assert!(!authenticated, "Resumed guest remains labelled guest");
                break;
            }
        }
        if connection < 2 {
            bridge.commands.send(Command::Reconnect).await.unwrap();
        }
    }
    drop(bridge);
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn rejected_token_is_redacted_and_does_not_fall_back_or_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(socket).await.unwrap();
        ws.send(Message::Text(
            json!({"method":"server","params":{"apron":8,"auth":["guest","token"]}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let req: Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(req["params"]["scheme"], "token");
        ws.send(Message::Text(
            json!({"id":req["id"],"error":{"code":-32001,"message":"synthetic-private-token"}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        // No guest auth on this socket, and no automatic reconnect after denial.
        if let Ok(Some(Ok(Message::Text(_)))) =
            tokio::time::timeout(Duration::from_secs(2), ws.next()).await
        {
            panic!("Unexpected fallback request");
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(1400), listener.accept())
                .await
                .is_err()
        );
    });
    let bridge = Bridge::start_with_token(
        format!("ws://{address}/ws"),
        Some(Secret::new("synthetic-private-token".into()).unwrap()),
    )
    .unwrap();
    loop {
        let event = next(&bridge).await;
        assert!(!format!("{event:?}").contains("synthetic-private-token"));
        if let Event::Failure { message, .. } = event {
            assert!(message.contains("Sign-in was not accepted"));
            break;
        }
    }
    server.await.unwrap();
}

#[tokio::test]
async fn connection_loss_during_auth_reconnects_without_exposing_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for attempt in 0..2 {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(socket).await.unwrap();
            ws.send(Message::Text(
                json!({"method":"server","params":{"apron":8,"auth":["token"]}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let request: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(request["params"]["token"], "synthetic-reconnect-token");
            if attempt == 1 {
                ws.send(Message::Text(json!({"id":request["id"],"error":{"code":-32001,"message":"synthetic-reconnect-token"}}).to_string().into())).await.unwrap();
            }
            ws.close(None).await.unwrap();
        }
    });
    let bridge = Bridge::start_with_token(
        format!("ws://{address}/ws"),
        Some(Secret::new("synthetic-reconnect-token".into()).unwrap()),
    )
    .unwrap();
    let mut reconnecting = false;
    loop {
        let event = next(&bridge).await;
        assert!(!format!("{event:?}").contains("synthetic-reconnect-token"));
        if let Event::State(state) = event {
            if state.starts_with("Disconnected") {
                reconnecting = true;
            }
            if state == "Sign-in required" {
                assert!(reconnecting);
                break;
            }
        }
    }
    server.await.unwrap();
}
