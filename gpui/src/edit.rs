//! Apron saves replace client fields; preserve the complete body and reply reference.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
pub fn payload(original: &Value, text: Option<&str>) -> Result<Value> {
    let id = original["message_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Message has no ID"))?;
    let room = original["room_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Message has no room"))?;
    ensure!(original["deleted"] != true, "Message was deleted");
    let Some(text) = text else {
        return Ok(json!({"message_id":id,"room_id":room,"deleted":true}));
    };
    ensure!(text.len() <= 32000, "Message exceeds 32,000 bytes");
    ensure!(
        !text.trim().is_empty()
            || original["body"]["embeds"]
                .as_array()
                .is_some_and(|a| !a.is_empty()),
        "Message cannot be empty"
    );
    let mut body = original["body"].clone();
    ensure!(body.is_object(), "Message body cannot be edited");
    body["text"] = text.into();
    let mut result = json!({"message_id":id,"room_id":room,"body":body});
    if let Some(reply) = original["reply_to"]["message_id"].as_str() {
        result["reply_to"] = json!({"message_id":reply});
    }
    // ext merges on the server and therefore need not be sent back.
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_preserves_body_fields_and_reply_but_not_server_fields() {
        let m = json!({"message_id":"1","log_id":"9","room_id":"r","from":{"user_id":"u"},"reply_to":{"message_id":"2","body":{"text":"embedded"}},"ext":{"a":1},"body":{"text":"old","format":"markdown","mentions":["v"],"embeds":[{"kind":"upload","url":"https://a/x"}]}});
        let p = payload(&m, Some("new")).unwrap();
        assert_eq!(p["body"]["mentions"], m["body"]["mentions"]);
        assert_eq!(p["body"]["embeds"], m["body"]["embeds"]);
        assert_eq!(p["reply_to"], json!({"message_id":"2"}));
        assert!(p.get("from").is_none());
        assert!(p.get("log_id").is_none());
        assert_eq!(
            payload(&m, None).unwrap(),
            json!({"message_id":"1","room_id":"r","deleted":true})
        );
    }
}
