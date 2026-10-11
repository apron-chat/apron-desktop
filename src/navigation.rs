//! Bounded Unicode subsequence matching for room titles and parent context.
pub fn score(query: &str, text: &str) -> Option<usize> {
    let query = query.trim().to_lowercase();
    let text = text.to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let mut chars = text.chars().enumerate();
    let mut cost = 0;
    let mut last = None;
    for target in query.chars().take(128) {
        let (pos, _) = chars.find(|(_, c)| *c == target)?;
        cost += last.map_or(pos, |prev| pos - prev - 1);
        last = Some(pos);
    }
    Some(cost)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_and_gaps() {
        assert!(score("eng", "Engineering") < score("eng", "Evening"));
        assert!(score("日本", "日本語").is_some());
        assert_eq!(score("zz", "General"), None);
    }
}
