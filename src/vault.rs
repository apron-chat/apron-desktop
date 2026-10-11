//! Optional Linux Secret Service via libsecret's helper. Never falls back to files.
use crate::auth::Secret;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn account(server: &str) -> String {
    format!("{:x}", Sha256::digest(server.as_bytes()))
}
async fn invoke(
    program: &str,
    verb: &str,
    server: &str,
    secret: Option<&Secret>,
) -> Result<Option<Secret>> {
    let operation = async {
        let mut command = tokio::process::Command::new(program);
        command.arg(verb);
        if verb == "store" {
            command.args(["--label=Apron native session"]);
        }
        let mut child=command.args(["application","apron-desktop","server",&account(server)])
            .stdin(if secret.is_some(){Stdio::piped()}else{Stdio::null()})
            .stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn()
            .context("Secret Service helper unavailable; install libsecret's secret-tool and configure a keyring")?;
        if let Some(secret) = secret {
            let mut input = child.stdin.take().context("Keyring input unavailable")?;
            input.write_all(secret.expose().as_bytes()).await?;
            input.shutdown().await?;
        }
        let mut data = zeroize::Zeroizing::new(Vec::new());
        child
            .stdout
            .take()
            .context("Keyring output unavailable")?
            .take(8194)
            .read_to_end(&mut data)
            .await?;
        ensure!(data.len() <= 8193, "Keyring returned an oversized token");
        ensure!(
            child.wait().await?.success(),
            "Keyring request failed or was cancelled; no plaintext fallback was used"
        );
        if verb == "lookup" {
            let text = std::str::from_utf8(&data)
                .context("Invalid keyring token")?
                .trim_end_matches('\n');
            Ok(Some(
                Secret::new(text.to_owned())
                    .map_err(|_| anyhow::anyhow!("Invalid keyring token"))?,
            ))
        } else {
            Ok(None)
        }
    };
    tokio::time::timeout(Duration::from_secs(20), operation)
        .await
        .context("Keyring request timed out")?
}
pub async fn load(server: &str) -> Result<Secret> {
    invoke("secret-tool", "lookup", server, None)
        .await?
        .context("No saved session")
}
pub async fn store(server: &str, secret: &Secret) -> Result<()> {
    invoke("secret-tool", "store", server, Some(secret))
        .await
        .map(|_| ())
}
pub async fn clear(server: &str) -> Result<()> {
    invoke("secret-tool", "clear", server, None)
        .await
        .map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_scoped_without_exposing_server() {
        assert_ne!(account("wss://a/ws"), account("wss://b/ws"));
        assert_eq!(account("wss://a/ws").len(), 64);
    }
    #[tokio::test]
    async fn unavailable_has_no_fallback() {
        assert!(
            invoke(
                "/nonexistent/apron-test-helper",
                "lookup",
                "wss://synthetic/ws",
                None
            )
            .await
            .is_err()
        );
    }
}
