use apron_gpui::model::{Message, grouped};
use serde_json::json;
fn message(id: u64, user: &str) -> Message {
    Message::parse(json!({"message_id":id.to_string(),"log_id":id.to_string(),"room_id":"general","from":{"user_id":user},"body":{"text":"example"}})).unwrap()
}
#[test]
fn author_runs_break_at_replies_identity_time_and_date_boundaries() {
    let first = message(86_400_000, "a");
    assert!(grouped(&first, &message(86_401_000, "a")));
    assert!(!grouped(&first, &message(86_401_000, "b")));
    assert!(!grouped(&first, &message(86_700_000, "a")));
    assert!(!grouped(&message(86_399_999, "a"), &first));
    let mut reply = message(86_401_000, "a");
    reply.raw["reply_to"] = json!({"message_id":"1"});
    assert!(!grouped(&first, &reply));
}
