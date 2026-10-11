//! Pure protocol policy; no polling or desktop-global input tracking.
pub fn optional_statuses(server: &serde_json::Value) -> Vec<String> {
    if !server["capabilities"]
        .as_array()
        .is_some_and(|v| v.iter().any(|c| c == "status"))
    {
        return vec![];
    }
    ["dnd", "invisible"]
        .into_iter()
        .filter(|value| {
            server["status"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == value))
        })
        .map(str::to_owned)
        .collect()
}
pub fn idle(enabled: bool, active: bool, elapsed: u64) -> Option<u64> {
    (enabled && (!active || elapsed >= 300)).then_some(elapsed.min(31_536_000))
}
/// Parent mutes inherit through nested threads. Cap traversal even for cyclic input.
pub fn muted(
    room: &str,
    rooms: &std::collections::BTreeMap<String, serde_json::Value>,
    mutes: &std::collections::BTreeMap<String, Option<std::time::Instant>>,
    now: std::time::Instant,
) -> bool {
    let active = |id: &str| {
        mutes
            .get(id)
            .is_some_and(|end| end.is_none_or(|end| end > now))
    };
    if active("") {
        return true;
    }
    let mut current = room;
    for _ in 0..=256 {
        if active(current) {
            return true;
        }
        let Some(parent) = rooms
            .get(current)
            .and_then(|r| r["parent_room_id"].as_str())
            .filter(|p| !p.is_empty())
        else {
            return false;
        };
        current = parent;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_mutes_expiry_and_malformed_cycles_are_bounded() {
        use std::{
            collections::BTreeMap,
            time::{Duration, Instant},
        };
        let now = Instant::now();
        let mut rooms = BTreeMap::from([
            (
                "child".into(),
                serde_json::json!({"parent_room_id":"thread"}),
            ),
            (
                "thread".into(),
                serde_json::json!({"parent_room_id":"general"}),
            ),
        ]);
        let mut mutes = BTreeMap::from([("general".into(), None)]);
        assert!(muted("child", &rooms, &mutes, now));
        mutes.insert("general".into(), Some(now - Duration::from_secs(1)));
        assert!(!muted("child", &rooms, &mutes, now));
        rooms.insert(
            "general".into(),
            serde_json::json!({"parent_room_id":"child"}),
        );
        assert!(!muted("child", &rooms, &mutes, now));
        mutes.insert("".into(), None);
        assert!(muted("missing", &rooms, &mutes, now));
    }
    #[test]
    fn advertised_values_and_idle_privacy() {
        assert_eq!(
            optional_statuses(
                &serde_json::json!({"capabilities":["status"],"status":["dnd","made-up"]})
            ),
            ["dnd"]
        );
        assert!(optional_statuses(&serde_json::json!({"status":["dnd"]})).is_empty());
        assert_eq!(idle(false, false, 600), None);
        assert_eq!(idle(true, false, 0), Some(0));
        assert_eq!(idle(true, true, 299), None);
        assert_eq!(idle(true, true, 300), Some(300));
    }
}
