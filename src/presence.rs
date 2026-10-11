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
#[cfg(test)]
mod tests {
    use super::*;
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
