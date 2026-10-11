//! Synthetic loopback only: ensure replacements cannot silently discard unknown sets.
use apron_gpui::transport::Client;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
#[tokio::test]
async fn toggle_preserves_own_set_from_later_history_page() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut ws = accept_async(listener.accept().await.unwrap().0)
            .await
            .unwrap();
        ws.send(Message::Text(
            json!({"method":"server","params":{"apron":8}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        for step in 0..3 {
            let raw = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let req: Value = serde_json::from_str(&raw).unwrap();
            let result = match step {
                0 => {
                    assert_eq!(req["method"], "history");
                    assert_eq!(req["params"]["after"], "10");
                    json!({"history_log_id":"1","latest_log_id":"30","last_log_id":"20","more":true})
                }
                1 => {
                    assert_eq!(req["params"]["after"], "21");
                    assert_eq!(req["params"]["before"], "30");
                    json!({"history_log_id":"1","latest_log_id":"30","more":false,"reactions":[{"message_id":"10","log_id":"30","reactions":[{"from":{"user_id":"me"},"emojis":["❤️"]}]}]})
                }
                _ => {
                    assert_eq!(req["method"], "reactions");
                    let set = req["params"]["emojis"].as_array().unwrap();
                    assert!(set.contains(&json!("❤️")));
                    assert!(set.contains(&json!("👍")));
                    json!({})
                }
            };
            ws.send(Message::Text(
                json!({"id":req["id"],"result":result}).to_string().into(),
            ))
            .await
            .unwrap();
        }
    });
    let mut c = Client::connect(&format!("ws://{address}/ws"), None)
        .await
        .unwrap();
    c.toggle_reaction("r", 10, "me", "👍").await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn expired_unknown_state_never_sends_a_write() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut ws = accept_async(listener.accept().await.unwrap().0)
            .await
            .unwrap();
        ws.send(Message::Text(
            json!({"method":"server","params":{"apron":8}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let req: Value =
            serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
        ws.send(Message::Text(json!({"id":req["id"],"result":{"history_log_id":"20","latest_log_id":"30","more":false}}).to_string().into())).await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(150), ws.next())
                .await
                .is_err()
        );
    });
    let mut c = Client::connect(&format!("ws://{address}/ws"), None)
        .await
        .unwrap();
    assert!(
        c.toggle_reaction("r", 10, "me", "👍")
            .await
            .unwrap_err()
            .to_string()
            .contains("incomplete")
    );
    server.await.unwrap();
}
