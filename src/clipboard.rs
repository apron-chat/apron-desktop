//! Explicit Wayland file offers only. Never interpret ordinary pasted text as paths.
use anyhow::{Context, Result, ensure};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;
pub fn parse_uris(text: &str) -> Result<Vec<PathBuf>> {
    ensure!(text.len() <= 65536, "Clipboard file list exceeds 64 KiB");
    let mut paths = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        ensure!(
            paths.len() < 4 && line.len() <= 16384,
            "Clipboard is limited to four file paths"
        );
        let url = url::Url::parse(line)?;
        ensure!(
            url.scheme() == "file"
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Clipboard contains a non-local file URI"
        );
        let path = url
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Clipboard file is not local"))?;
        ensure!(path.is_absolute(), "Clipboard file must be absolute");
        paths.push(path);
    }
    ensure!(!paths.is_empty(), "Clipboard file list is empty");
    Ok(paths)
}
async fn command(args: &[&str]) -> Result<String> {
    let mut child = tokio::process::Command::new("wl-paste")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut bytes = Vec::new();
    child
        .stdout
        .take()
        .context("Clipboard output unavailable")?
        .take(65537)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(bytes.len() <= 65536, "Clipboard file offer exceeds 64 KiB");
    ensure!(
        child.wait().await?.success(),
        "Clipboard offer is unavailable"
    );
    String::from_utf8(bytes).context("Clipboard file list is not UTF-8")
}
pub async fn file_offer() -> Result<Option<Vec<PathBuf>>> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let types = match command(&["--list-types"]).await {
            Ok(types) => types,
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Ok(None);
            }
            // Empty clipboard is also an ordinary no-file result.
            Err(_) => return Ok(None),
        };
        if types.lines().any(|s| s == "text/uri-list") {
            return parse_uris(&command(&["--no-newline", "--type", "text/uri-list"]).await?)
                .map(Some);
        }
        if types.lines().any(|s| s == "x-special/gnome-copied-files") {
            let data = command(&["--no-newline", "--type", "x-special/gnome-copied-files"]).await?;
            let (_, uris) = data.split_once('\n').context("Invalid copied-file offer")?;
            return parse_uris(uris).map(Some);
        }
        Ok(None)
    })
    .await
    .context("Clipboard file request timed out")?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uri_lists_are_local_bounded_and_percent_decoded() {
        assert_eq!(
            parse_uris("# comment\r\nfile:///tmp/a%20b.txt\r\n").unwrap(),
            [PathBuf::from("/tmp/a b.txt")]
        );
        for bad in [
            "https://example.test/f",
            "file://remote/tmp/f",
            "file:///tmp/f?secret=x",
            "ordinary text",
        ] {
            assert!(parse_uris(bad).is_err());
        }
        assert!(parse_uris(&"file:///tmp/f\n".repeat(5)).is_err());
    }
}
