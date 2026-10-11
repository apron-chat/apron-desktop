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
pub fn title(raw: &str) -> Result<String> {
    let title = raw.trim();
    ensure!(
        !title.is_empty()
            && title.len() <= 720
            && title.chars().count() <= 180
            && !title.chars().any(char::is_control)
            && crate::model::clean(title, 180) == title,
        "Use a title of 1–180 characters without control or directional override characters"
    );
    Ok(title.to_owned())
}
pub fn payload_with_titles(
    original: &Value,
    text: Option<&str>,
    titles: &[(usize, String)],
) -> Result<Value> {
    ensure!(
        titles.len() <= 4,
        "Edit at most four attachment titles at once"
    );
    let mut result = payload(original, text)?;
    if text.is_none() {
        return Ok(result);
    }
    for (index, value) in titles {
        let embed = result["body"]["embeds"]
            .as_array_mut()
            .and_then(|es| es.get_mut(*index))
            .ok_or_else(|| anyhow::anyhow!("Attachment is no longer present"))?;
        ensure!(
            embed["kind"] == "upload",
            "Only upload titles can be changed"
        );
        if embed["title"].as_str().unwrap_or("") != value {
            embed["title"] = title(value)?.into();
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles_preserve_embed_identity_urls_metadata_and_caption() {
        let original = json!({"message_id":"1","room_id":"r","reply_to":{"message_id":"2"},"body":{"text":"caption","mentions":["peer"],"embeds":[{"kind":"upload","embed_id":"e1","url":"https://synthetic/f","title":"old.png","og":{"image":{"url":"https://synthetic/thumb"}},"extra":"keep"}]}});
        let changed =
            payload_with_titles(&original, Some("caption"), &[(0, "new.png".into())]).unwrap();
        let mut expected = original["body"].clone();
        expected["embeds"][0]["title"] = json!("new.png");
        assert_eq!(changed["body"], expected);
        assert_eq!(changed["reply_to"], original["reply_to"]);
        for bad in ["", "  ", "x\n.png", "\u{202e}evil"] {
            assert!(title(bad).is_err());
        }
        assert!(title(&"a".repeat(181)).is_err());
    }
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
