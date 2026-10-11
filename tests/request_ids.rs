//! Regression for Apron §1.2 user-scoped request deduplication across connections.
use apron_gpui::transport::Client;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
#[tokio::test]
async fn fresh_operations_do_not_reuse_ids_across_same_user_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..3 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            ws.send(Message::Text(
                json!({"method":"server","params":{"apron":8,"auth":["guest"]}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            for index in 0..3 {
                let request: Value =
                    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                        .unwrap();
                if index > 0 {
                    assert!(
                        seen.insert(request["id"].as_str().unwrap().to_owned()),
                        "A new operation reused another connection's request ID"
                    );
                }
                ws.send(Message::Text(json!({"id":request["id"],"result":{"you":{"user_id":"same-synthetic-account"}}}).to_string().into())).await.unwrap();
            }
        }
    });
    for connection in 0..3 {
        let mut client = Client::connect(&url, None).await.unwrap();
        client.guest().await.unwrap();
        client
            .rpc(
                "me",
                json!({"name":format!("Synthetic connection {connection}")}),
            )
            .await
            .unwrap();
        client.rpc("room_list", json!({})).await.unwrap();
    }
    server.await.unwrap();
}
