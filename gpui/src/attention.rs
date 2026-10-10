//! Session-local attention, bounded independently of retained message history.
use std::collections::BTreeMap;
const ROOMS: usize = 256;
const PENDING: usize = 2000;
#[derive(Default)]
struct Room {
    read: u64,
    seen: u64,
    pending: BTreeMap<u64, bool>,
}
#[derive(Default)]
pub struct Attention {
    rooms: BTreeMap<String, Room>,
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
        r.read = r.read.max(through);
        r.seen = r.seen.max(through);
        r.pending.retain(|id, _| *id > through);
        changed
    }
    pub fn counts(&self, room: &str) -> (usize, usize) {
        self.rooms
            .get(room)
            .map(|r| (r.pending.len(), r.pending.values().filter(|v| **v).count()))
            .unwrap_or_default()
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
