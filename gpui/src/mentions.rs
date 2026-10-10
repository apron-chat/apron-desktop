//! Explicit mention selection backed by current room identities, never display-name guesses.
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub id: String,
    pub name: String,
}
pub fn members(room: &Value, users: &BTreeMap<String, Value>) -> Vec<Member> {
    let mut out = BTreeMap::new();
    if let Some(members) = room["members"].as_array() {
        for m in members.iter().take(512) {
            if let Some(id) = m["user_id"].as_str().filter(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && !id.chars().any(char::is_whitespace)
                    && crate::model::clean(id, 128) == *id
            }) {
                let name = m["name"]
                    .as_str()
                    .or_else(|| users.get(id).and_then(|u| u["name"].as_str()))
                    .unwrap_or(id);
                out.insert(
                    id.to_owned(),
                    Member {
                        id: id.into(),
                        name: crate::model::clean(name, 80),
                    },
                );
            }
        }
    }
    out.into_values().collect()
}
pub fn query(text: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    let before = text.get(..cursor)?;
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let query = before.get(start..)?.strip_prefix('@')?;
    if query.len() > 128 || query.contains('@') {
        return None;
    }
    Some((start..cursor, query.to_lowercase()))
}
pub fn candidates(members: &[Member], query: &str) -> Vec<Member> {
    members
        .iter()
        .filter(|m| m.id.to_lowercase().contains(query) || m.name.to_lowercase().contains(query))
        .take(6)
        .cloned()
        .collect()
}
pub fn present(text: &str, id: &str) -> bool {
    let marker = format!("@{id}");
    text.match_indices(&marker).any(|(i, _)| {
        let end = i + marker.len();
        (i == 0
            || text[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace() || "([{".contains(c)))
            && text[end..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || ".,!?:;)]}".contains(c))
    })
}
pub fn recipients(text: &str, selected: &BTreeSet<String>) -> Vec<String> {
    selected
        .iter()
        .filter(|id| present(text, id))
        .take(32)
        .cloned()
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn identities_are_not_guessed_and_text_alone_is_not_a_mention() {
        let members = members(
            &json!({"members":[{"user_id":"a","name":"Alex"},{"user_id":"b","name":"Alex"}]}),
            &BTreeMap::new(),
        );
        assert_eq!(candidates(&members, "alex").len(), 2);
        assert!(recipients("@a", &BTreeSet::new()).is_empty());
        assert_eq!(
            recipients("Hi @a!", &BTreeSet::from(["a".into()])),
            vec!["a"]
        );
        assert!(!present("@alice", "a"));
        assert!(!present("mail@a", "a"));
    }
    #[test]
    fn unicode_cursor_boundaries() {
        let t = "Hello 世界 @al";
        let (r, q) = query(t, t.len()).unwrap();
        assert_eq!(&t[r], "@al");
        assert_eq!(q, "al");
        assert!(query(t, 7).is_none());
    }
}
