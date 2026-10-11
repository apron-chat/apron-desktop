use apron_gpui::transport::Client;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
#[tokio::test]
async fn idle_uses_elapsed_seconds_and_falls_back_only_after_explicit_type_rejection() {
    for rejection in [
        None,
        Some("idle must be a boolean"),
        Some("Permission denied"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut ws = accept_async(listener.accept().await.unwrap().0)
                .await
                .unwrap();
            ws.send(Message::Text(
                json!({"method":"server","params":{"apron":8,"capabilities":["status"]}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let req: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            assert_eq!(req["params"]["idle"], 300);
            let reply = if let Some(message) = rejection {
                json!({"id":req["id"],"error":{"code":-32602,"message":message}})
            } else {
                json!({"id":req["id"],"result":{}})
            };
            ws.send(Message::Text(reply.to_string().into()))
                .await
                .unwrap();
            if rejection == Some("idle must be a boolean") {
                let req: Value =
                    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                        .unwrap();
                assert_eq!(req["params"]["idle"], true);
                ws.send(Message::Text(
                    json!({"id":req["id"],"result":{}}).to_string().into(),
                ))
                .await
                .unwrap();
            }
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next())
                    .await
                    .is_err()
            );
        });
        let mut client = Client::connect(&format!("ws://{address}/ws"), None)
            .await
            .unwrap();
        assert_eq!(
            client.set_idle(Some(300)).await.is_ok(),
            rejection != Some("Permission denied")
        );
        server.await.unwrap();
    }
}
