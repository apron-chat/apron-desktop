//! Bounded, log-ordered replacement sets, including reactions for unloaded messages.
use crate::model::{number, text};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const MAX_SETS: usize = 2048;
#[derive(Default)]
pub struct Reactions {
    sets: BTreeMap<(u64, String), (u64, String, BTreeSet<String>)>,
}
impl Reactions {
    pub fn apply(&mut self, record: &Value) {
        let (Some(id), Some(log), Some(room)) = (
            number(&record["message_id"]),
            number(&record["log_id"]),
            record["room_id"].as_str(),
        ) else {
            return;
        };
        if room.len() > 256 {
            return;
        }
        for entry in record["reactions"]
            .as_array()
            .into_iter()
            .flatten()
            .take(128)
        {
            let user = text(&entry["from"]["user_id"]);
            let Some(emojis) = entry["emojis"].as_array() else {
                continue;
            };
            // Reject an unrepresentable complete set rather than truncate it and overwrite it later.
            if user.is_empty()
                || user.len() > 256
                || emojis.len() > 16
                || emojis.iter().any(|e| {
                    e.as_str().is_none_or(|e| {
                        e.len() > 64
                            || e.is_empty()
                            || e.chars().any(char::is_control)
                            || crate::model::clean(e, 64) != e
                    })
                })
            {
                continue;
            }
            let key = (id, user.to_owned());
            if self.sets.get(&key).is_some_and(|old| old.0 >= log) {
                continue;
            }
            self.sets.insert(
                key,
                (
                    log,
                    room.to_owned(),
                    emojis
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect(),
                ),
            );
            while self.sets.len() > MAX_SETS {
                let key = self
                    .sets
                    .iter()
                    .min_by_key(|(_, v)| v.0)
                    .map(|(k, _)| k.clone())
                    .unwrap();
                self.sets.remove(&key);
            }
        }
    }
    pub fn own(&self, id: u64, user: &str) -> BTreeSet<String> {
        self.sets
            .get(&(id, user.to_owned()))
            .map(|v| v.2.clone())
            .unwrap_or_default()
    }
    pub fn totals(&self, id: u64) -> Vec<(String, usize)> {
        let mut totals = BTreeMap::new();
        for ((mid, _), (_, _, set)) in &self.sets {
            if *mid == id {
                for emoji in set {
                    *totals.entry(emoji.clone()).or_default() += 1;
                }
            }
        }
        totals.into_iter().take(16).collect()
    }
    pub fn records(&self) -> Vec<Value> {
        self.sets.iter().map(|((id,user),(log,room,set))|json!({"message_id":id.to_string(),"log_id":log.to_string(),"room_id":room,"reactions":[{"from":{"user_id":user},"emojis":set}]})).collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record(log: u64, user: &str, emojis: Value) -> Value {
        json!({"message_id":"10","log_id":log.to_string(),"room_id":"general","reactions":[{"from":{"user_id":user},"emojis":emojis}]})
    }
    #[test]
    fn replacement_empty_duplicates_and_stale() {
        let mut r = Reactions::default();
        r.apply(&record(2, "a", json!(["👍", "👍"])));
        r.apply(&record(3, "b", json!(["👍"])));
        assert_eq!(r.totals(10), vec![("👍".into(), 2)]);
        r.apply(&record(4, "a", json!([])));
        r.apply(&record(1, "a", json!(["❤️"])));
        assert!(r.own(10, "a").is_empty());
        assert_eq!(r.totals(10), vec![("👍".into(), 1)]);
        let mut copy = Reactions::default();
        for v in r.records() {
            copy.apply(&v);
        }
        assert_eq!(copy.totals(10), r.totals(10));
    }
    #[test]
    fn bounded_and_invalid_sets_are_not_partially_applied() {
        let mut r = Reactions::default();
        for i in 0..2200 {
            r.apply(&record(i, &i.to_string(), json!(["👍"])));
        }
        assert_eq!(r.sets.len(), MAX_SETS);
        r.apply(&record(3000, "bad", json!(["ok", "\u{001b}"])));
        assert!(r.own(10, "bad").is_empty());
    }
}

/// Completeness proof for an own replacement set. Missing/invalid is never empty.
#[derive(Default)]
pub struct OwnScan {
    latest: u64,
    own: Option<Option<BTreeSet<String>>>,
}
impl OwnScan {
    pub fn apply(&mut self, record: &Value, message: u64, user: &str) {
        if number(&record["message_id"]) != Some(message) {
            return;
        }
        let Some(log) = number(&record["log_id"]) else {
            return;
        };
        for entry in record["reactions"].as_array().into_iter().flatten() {
            if entry["from"]["user_id"] != user || log < self.latest {
                continue;
            }
            self.latest = log;
            self.own = Some(entry["emojis"].as_array().and_then(|values| {
                if values.len() > 16 {
                    return None;
                }
                values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|s| {
                                !s.is_empty()
                                    && s.len() <= 64
                                    && !s.chars().any(char::is_control)
                                    && crate::model::clean(s, 64) == *s
                            })
                            .map(str::to_owned)
                    })
                    .collect()
            }));
        }
    }
    pub fn finish(self, creation_retained: bool) -> anyhow::Result<BTreeSet<String>> {
        match self.own {
            Some(Some(set)) => Ok(set),
            None if creation_retained => Ok(BTreeSet::new()),
            _ => anyhow::bail!("Reaction state is incomplete. No reaction was changed."),
        }
    }
}
#[cfg(test)]
mod scan_tests {
    use super::*;
    #[test]
    fn unknown_empty_replacement_and_invalid_are_distinct() {
        assert!(OwnScan::default().finish(false).is_err());
        assert!(OwnScan::default().finish(true).unwrap().is_empty());
        let mut scan = OwnScan::default();
        let rec = |log, emojis| json!({"message_id":"10","log_id":log,"reactions":[{"from":{"user_id":"me"},"emojis":emojis}]});
        scan.apply(&rec("12", json!(["👍", "❤️"])), 10, "me");
        scan.apply(&rec("11", json!([])), 10, "me");
        assert_eq!(scan.finish(false).unwrap().len(), 2);
        let mut scan = OwnScan::default();
        scan.apply(&rec("12", json!(["👍"])), 10, "me");
        scan.apply(&rec("13", json!(["\u{001b}"])), 10, "me");
        assert!(scan.finish(true).is_err());
    }
}
