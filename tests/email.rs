use apron_gpui::{
    auth::Secret,
    email::{Request, Session},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{accept_async, tungstenite::Message};
#[tokio::test]
async fn proposal_and_code_use_same_unauthenticated_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut ws = accept_async(listener.accept().await.unwrap().0)
            .await
            .unwrap();
        ws.send(Message::Text(
            json!({"method":"server","params":{"apron":8,"auth":["email"]}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        for step in 0..2 {
            let req: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            assert_eq!(req["method"], "auth");
            assert_eq!(req["params"]["scheme"], "email");
            let result = if step == 0 {
                assert_eq!(req["params"]["email"], "synthetic@example.invalid");
                json!({})
            } else {
                assert_eq!(req["params"]["token"], "123456");
                json!({"you":{"user_id":"synthetic"},"token":"synthetic-bearer"})
            };
            ws.send(Message::Text(
                json!({"id":req["id"],"result":result}).to_string().into(),
            ))
            .await
            .unwrap();
        }
    });
    let session = Session::start(format!("ws://{address}/ws"));
    session
        .commands
        .send(Request::Propose("synthetic@example.invalid".into()))
        .await
        .unwrap();
    assert!(session.results.recv().await.unwrap().unwrap().is_none());
    session
        .commands
        .send(Request::Approve(Secret::new("123456".into()).unwrap()))
        .await
        .unwrap();
    assert!(session.results.recv().await.unwrap().unwrap().is_some());
    server.await.unwrap();
}
