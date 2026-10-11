use apron_gpui::transport::{Bridge, Command, Event};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{accept_async, tungstenite::Message};
#[tokio::test]
async fn bounded_recovery_pages_forward_and_reports_retention_gap() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut ws = accept_async(listener.accept().await.unwrap().0)
            .await
            .unwrap();
        ws.send(Message::Text(json!({"method":"server","params":{"apron":8,"auth":["guest"],"capabilities":["history","rooms"]}}).to_string().into())).await.unwrap();
        let mut histories = 0;
        while let Some(Ok(Message::Text(raw))) = ws.next().await {
            let req: Value = serde_json::from_str(&raw).unwrap();
            let result = match req["method"].as_str().unwrap() {
                "auth" => json!({"you":{"user_id":"me"}}),
                "room_list" => json!({"joined":[{"room_id":"r"}]}),
                "history" => {
                    histories += 1;
                    if histories == 1 {
                        assert_eq!(req["params"]["after"], "11");
                        json!({"history_log_id":"15","latest_log_id":"30","last_log_id":"20","more":true})
                    } else {
                        assert_eq!(req["params"]["after"], "21");
                        assert_eq!(req["params"]["before"], "30");
                        json!({"history_log_id":"15","latest_log_id":"30","last_log_id":"30","more":false,"messages":[{"room_id":"r","message_id":"25","log_id":"25","from":{"user_id":"other"},"body":{"text":"synthetic"}}]})
                    }
                }
                _ => panic!("Unexpected request"),
            };
            ws.send(Message::Text(
                json!({"id":req["id"],"result":result}).to_string().into(),
            ))
            .await
            .unwrap();
            if histories == 2 {
                break;
            }
        }
    });
    let bridge = Bridge::start(format!("ws://{address}/ws")).unwrap();
    let next = || tokio::time::timeout(std::time::Duration::from_secs(5), bridge.events.recv());
    while !matches!(next().await.unwrap().unwrap(), Event::Ready { .. }) {}
    bridge
        .commands
        .send(Command::Recover {
            epoch: 7,
            cursors: std::collections::BTreeMap::from([("r".into(), 10)]),
        })
        .await
        .unwrap();
    let mut pages = 0;
    let mut gap = false;
    while pages < 2 {
        match next().await.unwrap().unwrap() {
            Event::Recovered { epoch, room, .. } => {
                assert_eq!(epoch, 7);
                assert_eq!(room, "r");
                pages += 1;
            }
            Event::RecoveryGap { .. } => gap = true,
            _ => {}
        }
    }
    assert!(gap);
    drop(bridge);
    server.await.unwrap();
}
