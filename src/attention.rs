//! Session-local attention, bounded independently of retained message history.
use std::collections::BTreeMap;
const ROOMS: usize = 256;
const PENDING: usize = 2000;
#[derive(Default)]
struct Room {
    read: u64,
    baseline: bool,
    seen: u64,
    pending: BTreeMap<u64, bool>,
}
#[derive(Default)]
pub struct Attention {
    rooms: BTreeMap<String, Room>,
    evidence: BTreeMap<String, BTreeMap<u64, (u64, bool)>>,
}
impl Attention {
    pub fn receive(
        &mut self,
        room: &str,
        id: u64,
        own: bool,
        mention: bool,
        newly_mentioned: bool,
    ) {
        if room.len() > 256 {
            return;
        }
        if !self.rooms.contains_key(room) && self.rooms.len() >= ROOMS {
            return;
        }
        let state = self.rooms.entry(room.into()).or_default();
        let new = id > state.seen;
        state.seen = state.seen.max(id);
        if !own && ((new && id > state.read) || newly_mentioned) {
            state.pending.insert(id, mention);
        }
        // An edit can add/remove a mention without double-counting a message.
        if let Some(value) = state.pending.get_mut(&id) {
            *value = mention;
        }
        while self.rooms.values().map(|r| r.pending.len()).sum::<usize>() > PENDING {
            let oldest = self
                .rooms
                .iter()
                .filter_map(|(name, r)| {
                    r.pending
                        .first_key_value()
                        .map(|(id, _)| (name.clone(), *id))
                })
                .min_by_key(|(_, id)| *id);
            if let Some((name, id)) = oldest {
                self.rooms.get_mut(&name).unwrap().pending.remove(&id);
            } else {
                break;
            }
        }
    }
    pub fn remove(&mut self, room: &str, id: u64) {
        if let Some(r) = self.rooms.get_mut(room) {
            r.pending.remove(&id);
        }
    }
    pub fn mark_read(&mut self, room: &str, through: u64) -> bool {
        if room.len() > 256 {
            return false;
        }
        if !self.rooms.contains_key(room) && self.rooms.len() >= ROOMS {
            return false;
        }
        let r = self.rooms.entry(room.into()).or_default();
        let changed = through > r.read || r.pending.range(..=through).next().is_some();
        r.baseline = true;
        r.read = r.read.max(through);
        r.seen = r.seen.max(through);
        r.pending.retain(|id, _| *id > through);
        changed
    }
    pub fn cursors(&self) -> BTreeMap<String, u64> {
        self.rooms
            .iter()
            .filter(|(_, r)| r.baseline)
            .map(|(id, r)| (id.clone(), r.read))
            .collect()
    }
    pub fn restore(&mut self, cursors: &BTreeMap<String, u64>) {
        for (room, id) in cursors.iter().take(ROOMS) {
            self.mark_read(room, *id);
        }
    }
    /// Reconcile fetched snapshots against a known read cursor, never count a fresh
    /// installation's entire history as unread. Counters cover retained/fetched evidence.
    pub fn history(
        &mut self,
        room: &str,
        messages: &[std::sync::Arc<crate::model::Message>],
        you: &str,
    ) {
        if room.len() > 256 || (!self.rooms.contains_key(room) && self.rooms.len() >= ROOMS) {
            return;
        }
        let state = self.rooms.entry(room.into()).or_default();
        if !state.baseline {
            let latest = messages.iter().map(|m| m.id).max().unwrap_or(0);
            state.read = state
                .pending
                .first_key_value()
                .map(|(id, _)| id.saturating_sub(1).min(latest))
                .unwrap_or(latest);
            state.baseline = true;
        }
        let read = state.read;
        for m in messages {
            if self
                .evidence
                .get(room)
                .and_then(|v| v.get(&m.id))
                .is_some_and(|(log, _)| *log > m.log)
            {
                continue;
            }

            let own = m.raw["from"]["user_id"] == you;
            if m.deleted() || own {
                self.remove(room, m.id);
                continue;
            }
            let mentioned = m.raw["body"]["mentions"]
                .as_array()
                .is_some_and(|v| v.iter().any(|u| u == you));
            let added = self.remember(room, m.id, m.log, mentioned);
            // This is a snapshot of an unread message, including older fetched pages.
            if m.id <= read {
                if added {
                    self.receive(room, m.id, false, true, true);
                }
                if !mentioned {
                    self.remove(room, m.id);
                }
                continue;
            }
            self.receive(room, m.id, false, mentioned, true);
        }
    }
    pub fn pending(&self) -> BTreeMap<String, BTreeMap<u64, bool>> {
        self.rooms
            .iter()
            .filter(|(_, r)| !r.pending.is_empty())
            .map(|(room, r)| (room.clone(), r.pending.clone()))
            .collect()
    }
    pub fn restore_pending(&mut self, pending: &BTreeMap<String, BTreeMap<u64, bool>>) {
        if pending.len() > ROOMS || pending.values().map(BTreeMap::len).sum::<usize>() > PENDING {
            return;
        }
        for (room, entries) in pending {
            if room.len() <= 256 && (self.rooms.len() < ROOMS || self.rooms.contains_key(room)) {
                self.rooms.entry(room.clone()).or_default().pending = entries.clone();
            }
        }
    }
    pub fn evidence(&self) -> BTreeMap<String, BTreeMap<u64, (u64, bool)>> {
        self.evidence.clone()
    }
    pub fn restore_evidence(&mut self, value: BTreeMap<String, BTreeMap<u64, (u64, bool)>>) {
        if value.len() <= ROOMS && value.values().map(BTreeMap::len).sum::<usize>() <= PENDING {
            self.evidence = value;
        }
    }
    fn remember(&mut self, room: &str, id: u64, log: u64, mentioned: bool) -> bool {
        let entries = self.evidence.entry(room.to_owned()).or_default();
        let old = entries.get(&id).copied();
        if old.is_some_and(|(prev, _)| prev >= log) {
            return false;
        }
        entries.insert(id, (log, mentioned));
        let added = old.is_some_and(|(_, was)| !was && mentioned);
        while self.evidence.values().map(BTreeMap::len).sum::<usize>() > PENDING {
            let oldest = self
                .evidence
                .iter()
                .flat_map(|(r, entries)| {
                    entries
                        .iter()
                        .map(move |(id, (log, _))| (r.clone(), *id, *log))
                })
                .min_by_key(|v| v.2);
            if let Some((r, id, _)) = oldest {
                self.evidence.get_mut(&r).unwrap().remove(&id);
            } else {
                break;
            }
        }
        added
    }
    pub fn counts(&self, room: &str) -> (usize, usize) {
        self.rooms
            .get(room)
            .map(|r| (r.pending.len(), r.pending.values().filter(|v| **v).count()))
            .unwrap_or_default()
    }
    pub fn first_mention(&self, room: &str) -> Option<u64> {
        self.rooms
            .get(room)?
            .pending
            .iter()
            .find_map(|(id, mention)| mention.then_some(*id))
    }
    pub fn first_unread(&self, room: &str) -> Option<u64> {
        self.rooms
            .get(room)?
            .pending
            .first_key_value()
            .map(|(id, _)| *id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dedup_mentions_reads_and_edits() {
        let mut a = Attention::default();
        a.receive("r", 10, false, true, false);
        a.receive("r", 10, false, true, false);
        assert_eq!(a.counts("r"), (1, 1));
        a.receive("r", 10, false, false, false);
        assert_eq!(a.counts("r"), (1, 0));
        a.mark_read("r", 10);
        a.receive("r", 10, false, true, true);
        assert_eq!(a.counts("r"), (1, 1));
        a.mark_read("r", 10);
        a.receive("r", 9, false, false, false);
        a.receive("r", 11, true, false, false);
        assert_eq!(a.counts("r"), (0, 0));
        a.receive("r", 12, false, true, false);
        a.remove("r", 12);
        assert_eq!(a.counts("r"), (0, 0));
    }
    #[test]
    fn restored_cursor_reconciles_offline_messages_without_repeating_read_mentions() {
        use crate::model::Store;
        let mut store = Store::default();
        for id in 1..=4 {
            store.apply(serde_json::json!({"message_id":id.to_string(),"log_id":id.to_string(),"room_id":"r","from":{"user_id":"other"},"body":{"text":"synthetic","mentions":["you"]}}));
        }
        let mut fresh = Attention::default();
        fresh.history("r", &store.rows("r"), "you");
        assert_eq!(fresh.counts("r"), (0, 0));
        let mut restored = Attention::default();
        restored.restore(&BTreeMap::from([("r".into(), 2)]));
        restored.history("r", &store.rows("r"), "you");
        assert_eq!(restored.counts("r"), (2, 2));
        restored.history("r", &store.rows("r"), "you");
        assert_eq!(restored.counts("r"), (2, 2));
        restored.mark_read("r", 4);
        restored.history("r", &store.rows("r"), "you");
        assert_eq!(restored.counts("r"), (0, 0));
    }
    #[test]
    fn saved_evidence_recovers_old_edit_mentions_once() {
        use crate::model::Message;
        let msg = |log, mentions| {
            std::sync::Arc::new(Message::parse(serde_json::json!({"message_id":"10","log_id":log,"room_id":"r","from":{"user_id":"other"},"body":{"mentions":mentions}})).unwrap())
        };
        let mut a = Attention::default();
        a.history("r", &[msg("10", serde_json::json!([]))], "you");
        let mut b = Attention::default();
        b.restore(&a.cursors());
        b.restore_evidence(a.evidence());
        b.history("r", &[msg("20", serde_json::json!(["you"]))], "you");
        assert_eq!(b.counts("r"), (1, 1));
        let mut restarted = Attention::default();
        restarted.restore(&b.cursors());
        restarted.restore_evidence(b.evidence());
        restarted.restore_pending(&b.pending());
        restarted.history("r", &[msg("20", serde_json::json!(["you"]))], "you");
        assert_eq!(restarted.counts("r"), (1, 1));
        b.mark_read("r", 10);
        b.history("r", &[msg("20", serde_json::json!(["you"]))], "you");
        assert_eq!(b.counts("r"), (0, 0));
    }
    #[test]
    fn memory_is_bounded() {
        let mut a = Attention::default();
        for id in 1..=3000 {
            a.receive("r", id, false, false, false);
        }
        assert_eq!(a.counts("r").0, PENDING);
        for id in 0..1000 {
            a.receive(&format!("room-{id}"), 4000, false, false, false);
        }
        assert!(a.rooms.len() <= ROOMS);
        assert!(a.rooms.values().map(|r| r.pending.len()).sum::<usize>() <= PENDING);
    }
}
