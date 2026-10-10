//! Bounded protocol state. No GPUI, filesystem or runtime dependencies.
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
pub const PAGE: usize = 50;
pub const MAX_MESSAGES: usize = 2000;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FRAME: usize = 2 * 1024 * 1024;

pub fn number(value: &Value) -> Option<u64> {
    value.as_str()?.parse().ok()
}
pub fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
pub fn clean(raw: &str, limit: usize) -> String {
    raw.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .filter(|c| !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .take(limit)
        .collect()
}
/// Strip raw HTML and image syntax before GPUI parses Markdown; links remain inert in UI.
pub fn display_text(raw: &str, markdown: bool) -> String {
    let raw = clean(raw, 32000);
    if !markdown {
        return raw
            .chars()
            .flat_map(|c| {
                if "\\`*_{}[]<>()#+-.!|".contains(c) {
                    vec!['\\', c]
                } else {
                    vec![c]
                }
            })
            .collect();
    }
    use pulldown_cmark::{Event, Tag};
    let mut cuts = Vec::new();
    for (event, range) in pulldown_cmark::Parser::new(&raw).into_offset_iter() {
        if matches!(
            event,
            Event::Html(_) | Event::InlineHtml(_) | Event::Start(Tag::Image { .. })
        ) {
            cuts.push(range);
        }
    }
    cuts.sort_by_key(|r| r.start);
    let mut result = String::new();
    let mut end = 0;
    for range in cuts {
        if range.start >= end {
            result.push_str(&raw[end..range.start]);
            result.push_str(" [media omitted] ");
            end = range.end;
        }
    }
    result.push_str(&raw[end..]);
    result
}
#[derive(Clone, Debug)]
pub struct Message {
    pub id: u64,
    pub log: u64,
    pub room: String,
    pub raw: Value,
    pub display: Arc<str>,
    pub bytes: usize,
}
impl Message {
    pub fn parse(raw: Value) -> Option<Self> {
        let id = number(&raw["message_id"])?;
        let log = number(&raw["log_id"])?;
        let room = raw["room_id"].as_str()?.to_string();
        if room.len() > 256 {
            return None;
        }
        let bytes = serde_json::to_vec(&raw).ok()?.len();
        if bytes > MAX_FRAME {
            return None;
        }
        let display = display_text(
            text(&raw["body"]["text"]),
            text(&raw["body"]["format"]) == "markdown",
        )
        .into();
        Some(Self {
            id,
            log,
            room,
            raw,
            display,
            bytes,
        })
    }
    pub fn deleted(&self) -> bool {
        self.raw["deleted"].as_bool().unwrap_or(false)
    }
}
#[derive(Default)]
pub struct Store {
    messages: BTreeMap<u64, Arc<Message>>,
    bytes: usize,
}
impl Store {
    pub fn apply(&mut self, raw: Value) -> bool {
        let Some(message) = Message::parse(raw) else {
            return false;
        };
        if self.messages.get(&message.id).is_some_and(|old| {
            old.log > message.log || (old.log == message.log && !message.deleted())
        }) {
            return false;
        }
        if let Some(old) = self.messages.insert(message.id, Arc::new(message.clone())) {
            self.bytes -= old.bytes;
        }
        self.bytes += message.bytes;
        while self.messages.len() > MAX_MESSAGES || self.bytes > MAX_BYTES {
            if let Some((_, old)) = self.messages.pop_first() {
                self.bytes -= old.bytes;
            } else {
                break;
            }
        }
        true
    }
    pub fn rows(&self, room: &str) -> Vec<Arc<Message>> {
        self.messages
            .values()
            .filter(|m| m.room == room && !m.deleted())
            .cloned()
            .collect()
    }
    pub fn get(&self, id: u64) -> Option<&Arc<Message>> {
        self.messages.get(&id)
    }
    pub fn len(&self) -> usize {
        self.messages.len()
    }
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}
/// Ordered structural changes preserve untouched row anchors during prepend and eviction.
pub fn splices(old: &[Arc<Message>], new: &[Arc<Message>]) -> Vec<(std::ops::Range<usize>, usize)> {
    let (mut i, mut j, mut position) = (0, 0, 0);
    let mut result = Vec::new();
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i].id == new[j].id {
            i += 1;
            j += 1;
            position += 1;
            continue;
        }
        let start = position;
        let (mut removed, mut inserted) = (0, 0);
        while i < old.len() || j < new.len() {
            if i < old.len() && j < new.len() && old[i].id == new[j].id {
                break;
            }
            if j == new.len() || (i < old.len() && old[i].id < new[j].id) {
                i += 1;
                removed += 1;
            } else {
                j += 1;
                inserted += 1;
            }
        }
        result.push((start..start + removed, inserted));
        position += inserted;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn m(id: u64, log: u64) -> Value {
        json!({"message_id":id.to_string(),"log_id":log.to_string(),"room_id":"r","body":{"text":"hello"}})
    }
    #[test]
    fn replay_move_delete_and_bounds() {
        let mut s = Store::default();
        s.apply(m(10, 20));
        assert!(!s.apply(m(10, 19)));
        let mut moved = m(10, 21);
        moved["room_id"] = json!("other");
        s.apply(moved);
        assert!(s.rows("r").is_empty());
        let mut del = m(10, 22);
        del["deleted"] = json!(true);
        s.apply(del);
        assert!(s.rows("other").is_empty());
        for n in 100..2300 {
            s.apply(m(n, n));
        }
        assert_eq!(s.len(), MAX_MESSAGES);
        assert!(s.bytes() <= MAX_BYTES);
    }
    #[test]
    fn byte_limit_evicts_before_count_limit_and_rejects_large_frames() {
        let mut store = Store::default();
        for id in 1..=12 {
            let mut message = m(id, id);
            message["body"]["text"] = json!("x".repeat(750_000));
            store.apply(message);
        }
        assert!(store.len() < 12);
        assert!(store.bytes() <= MAX_BYTES);
        assert!(store.rows("r").first().unwrap().id > 1);
        let mut oversized = m(99, 99);
        oversized["body"]["text"] = json!("x".repeat(MAX_FRAME));
        assert!(!store.apply(oversized));
    }
    #[test]
    fn splice_preserves_survivor_ids() {
        let rows = |ids: &[u64]| {
            ids.iter()
                .map(|n| Arc::new(Message::parse(m(*n, *n)).unwrap()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            splices(&rows(&[3, 4]), &rows(&[1, 2, 3, 4])),
            vec![(0..0, 2)]
        );
        assert_eq!(
            splices(&rows(&[1, 2, 3]), &rows(&[2, 3, 4])),
            vec![(0..1, 0), (2..2, 1)]
        );
    }
    #[test]
    fn untrusted_display_has_no_html_images_controls() {
        let s = display_text(
            "Hi ![alt](https://x.invalid/a) <img src='x'> **bold**\u{202e}",
            true,
        );
        assert!(!s.contains("https://"));
        assert!(!s.contains("<img"));
        assert!(s.contains("**bold**"));
        assert!(!s.contains('\u{202e}'));
        assert_eq!(display_text("*plain*", false), "\\*plain\\*");
    }
}

/// Apply room-list snapshots and room_update deltas without changing membership on rename.
pub fn update_rooms(
    rooms: &mut std::collections::BTreeMap<String, serde_json::Value>,
    list: &serde_json::Value,
) {
    for key in ["joined", "not_joined", "updated"] {
        if let Some(records) = list[key].as_array() {
            for room in records.iter().take(256) {
                if let Some(id) = room["room_id"].as_str().filter(|id| id.len() <= 256) {
                    let old = rooms.get(id);
                    if key == "updated"
                        && old.is_some_and(|old| number(&old["log_id"]) > number(&room["log_id"]))
                    {
                        continue;
                    }
                    let joined = if key == "updated" {
                        old.is_some_and(|r| r["_joined"] == true)
                    } else {
                        key == "joined"
                    };
                    let mut record = room.clone();
                    // Members are an optional delivery projection, not room metadata.
                    if let Some(old) = old {
                        if record["members"].is_null() && old["members"].is_array() {
                            record["members"] = old["members"].clone();
                        }
                        if old["_membership_logs"].is_object() {
                            record["_membership_logs"] = old["_membership_logs"].clone();
                        }
                    }
                    record["_joined"] = serde_json::Value::Bool(joined);
                    rooms.insert(id.into(), record);
                }
            }
        }
    }
    if let Some(deltas) = list["memberships"].as_array() {
        for delta in deltas.iter().take(256) {
            let Some(room) = delta["room_id"].as_str().and_then(|id| rooms.get_mut(id)) else {
                continue;
            };
            let log = number(&delta["log_id"]).unwrap_or(0);
            let mut members: std::collections::BTreeMap<String, Value> = room["members"]
                .as_array()
                .into_iter()
                .flatten()
                .take(512)
                .filter_map(|m| m["user_id"].as_str().map(|id| (id.to_owned(), m.clone())))
                .collect();
            for entry in delta["members"].as_array().into_iter().flatten().take(512) {
                let Some(id) = entry["user"]["user_id"].as_str().filter(|s| s.len() <= 128) else {
                    continue;
                };
                let prior = number(&room["_membership_logs"][id])
                    .unwrap_or(number(&room["log_id"]).unwrap_or(0));
                if log < prior {
                    continue;
                }
                if room["_membership_logs"].is_null() {
                    room["_membership_logs"] = serde_json::json!({});
                }
                if room["_membership_logs"]
                    .as_object()
                    .is_some_and(|m| m.len() >= 512 && !m.contains_key(id))
                {
                    continue;
                }
                room["_membership_logs"][id] = Value::from(log.to_string());
                if entry["joined"] == true && (members.len() < 512 || members.contains_key(id)) {
                    members.insert(id.to_owned(), entry["user"].clone());
                } else if entry["joined"] == false {
                    members.remove(id);
                }
            }
            room["members"] = Value::Array(members.into_values().collect());
        }
    }
    if let Some(left) = list["left"].as_array() {
        for entry in left.iter().take(256) {
            if let Some(room) = entry["room_id"].as_str().and_then(|id| rooms.get_mut(id)) {
                room["_joined"] = serde_json::Value::Bool(false);
            }
        }
    }
    while rooms.len() > 256 {
        rooms.pop_last();
    }
}

#[cfg(test)]
mod room_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn membership_deltas_use_per_identity_clocks() {
        let mut rooms = std::collections::BTreeMap::new();
        update_rooms(
            &mut rooms,
            &json!({"joined":[{"room_id":"r","log_id":"10","members":[{"user_id":"a"}]}]}),
        );
        for (log, joined) in [(12, false), (11, true)] {
            update_rooms(
                &mut rooms,
                &json!({"memberships":[{"room_id":"r","log_id":log.to_string(),"members":[{"user":{"user_id":"a"},"joined":joined}]}]}),
            );
        }
        assert!(rooms["r"]["members"].as_array().unwrap().is_empty());
        update_rooms(
            &mut rooms,
            &json!({"memberships":[{"room_id":"r","log_id":"11","members":[{"user":{"user_id":"b","name":"Bee"},"joined":true}]}]}),
        );
        assert_eq!(rooms["r"]["members"][0]["user_id"], "b");
    }
    #[test]
    fn room_deltas_preserve_membership_and_reject_stale_rename() {
        let mut rooms = std::collections::BTreeMap::new();
        update_rooms(
            &mut rooms,
            &json!({"joined":[{"room_id":"r","title":"Before","log_id":"10"}]}),
        );
        update_rooms(
            &mut rooms,
            &json!({"updated":[{"room_id":"r","title":"After","log_id":"11"},{"room_id":"thread","parent_room_id":"r","title":"New thread","log_id":"12"}]}),
        );
        assert_eq!(rooms["r"]["title"], "After");
        assert_eq!(rooms["r"]["_joined"], true);
        assert_eq!(rooms["thread"]["_joined"], false);
        update_rooms(
            &mut rooms,
            &json!({"updated":[{"room_id":"r","title":"Stale","log_id":"9"}]}),
        );
        assert_eq!(rooms["r"]["title"], "After");
        update_rooms(&mut rooms, &json!({"left":[{"room_id":"r"}]}));
        assert_eq!(rooms["r"]["_joined"], false);
    }
}

/// Compact adjacent messages only when identity, room and a short time window agree.
/// Replies retain their own author heading; never group across the UTC date boundary.
pub fn grouped(previous: &Message, current: &Message) -> bool {
    !text(&current.raw["from"]["user_id"]).is_empty()
        && previous.raw["from"]["user_id"] == current.raw["from"]["user_id"]
        && previous.room == current.room
        && current.raw["reply_to"].is_null()
        && current.id.saturating_sub(previous.id) < 300_000
        && current.id / 86_400_000 == previous.id / 86_400_000
}
