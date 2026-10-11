//! Explicit external-link candidates. Never auto-open or fetch previews.
pub fn candidates(text: &str) -> Vec<String> {
    let mut urls = std::collections::BTreeSet::new();
    for event in pulldown_cmark::Parser::new(text) {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) = event
            && let Some(url) = safe(&dest_url)
        {
            urls.insert(url);
        }
        if urls.len() >= 16 {
            break;
        }
    }
    for word in text.split_whitespace().take(2048) {
        if urls.len() >= 16 {
            break;
        }
        if let Some(url) = safe(word.trim_end_matches(['.', ',', ')'])) {
            urls.insert(url);
        }
    }
    urls.into_iter().collect()
}
pub fn safe(raw: &str) -> Option<String> {
    if raw.len() > 2048 || raw.chars().any(char::is_control) {
        return None;
    }
    let url = url::Url::parse(raw).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_http_links() {
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "https://user:password@example.com",
        ] {
            assert!(safe(bad).is_none());
        }
        assert_eq!(
            candidates("[site](https://example.invalid) https://example.invalid").len(),
            1
        );
    }
}
