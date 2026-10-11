//! Bearer material exists only in the live session; Debug never exposes it.
use anyhow::{Result, ensure};
use zeroize::Zeroizing;
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(value: String) -> Result<Self> {
        ensure!(
            !value.is_empty() && value.len() <= 8192 && !value.chars().any(char::is_control),
            "Enter a token of 1–8192 bytes without control characters"
        );
        Ok(Self(Zeroizing::new(value)))
    }
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([redacted])")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_redacted_and_bounded() {
        let token = Secret::new("synthetic-token-for-unit-test".into()).unwrap();
        assert_eq!(format!("{token:?}"), "Secret([redacted])");
        assert!(Secret::new(String::new()).is_err());
        assert!(Secret::new("x".repeat(8193)).is_err());
        assert!(Secret::new("x\ny".into()).is_err());
    }
}
