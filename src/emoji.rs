//! Offline Unicode emoji catalogue. Search results and input are bounded.
pub fn search(query: &str) -> Vec<&'static emojis::Emoji> {
    if query.len() > 128 {
        return vec![];
    }
    let query = query.trim().to_lowercase();
    if let Some(exact) =
        emojis::get(&query).or_else(|| emojis::get_by_shortcode(query.trim_matches(':')))
    {
        return exact
            .skin_tones()
            .map(|v| v.take(32).collect())
            .unwrap_or_else(|| vec![exact]);
    }
    emojis::iter()
        .filter(|e| {
            e.name().to_lowercase().contains(&query) || e.shortcodes().any(|s| s.contains(&query))
        })
        .take(4096)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_sequences_skin_tones_and_bounds() {
        assert!(search("rocket").iter().any(|e| e.as_str() == "🚀"));
        assert!(search("👍🏽").iter().any(|e| e.as_str() == "👍🏽"));
        assert!(search("👩‍💻").iter().any(|e| e.as_str() == "👩‍💻"));
        assert!(search(&"x".repeat(129)).is_empty());
        assert!(search("").len() <= 4096);
    }
}
