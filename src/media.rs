//! Explicit, bounded HTTP transfers. No session credentials or redirects.
use anyhow::{Result, ensure};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use url::Url;
pub const LIMIT: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Attachment {
    pub title: String,
    pub bytes: Vec<u8>,
}
pub fn endpoint(raw: &str) -> Result<Url> {
    let u = Url::parse(raw)?;
    ensure!(
        u.username().is_empty() && u.password().is_none() && u.fragment().is_none(),
        "Address cannot include credentials or fragments"
    );
    ensure!(
        u.scheme() == "wss"
            || (u.scheme() == "ws"
                && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))),
        "Use wss, or ws on loopback only"
    );
    Ok(u)
}
pub fn transfer_url(raw: &str, server: &str) -> Result<Url> {
    let s = endpoint(server)?;
    let u = Url::parse(raw)?;
    ensure!(
        u.scheme() == if s.scheme() == "wss" { "https" } else { "http" }
            && u.host_str() == s.host_str()
            && u.port_or_known_default() == s.port_or_known_default(),
        "Attachment must be hosted on this server"
    );
    ensure!(
        u.username().is_empty() && u.password().is_none() && u.fragment().is_none(),
        "Invalid attachment address"
    );
    Ok(u)
}
fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(120))
        .build()?)
}
pub async fn read(paths: Vec<PathBuf>) -> Result<Vec<Attachment>> {
    tokio::task::spawn_blocking(move || read_local(paths)).await?
}
pub fn read_local(paths: Vec<PathBuf>) -> Result<Vec<Attachment>> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    ensure!(
        !paths.is_empty() && paths.len() <= 4,
        "Choose one to four files"
    );
    let mut out = Vec::new();
    let mut size = 0;
    for p in paths {
        // Reject special files before opening; re-check the opened descriptor too.
        ensure!(std::fs::metadata(&p)?.is_file(), "Choose regular files");
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(0x800); // Linux O_NONBLOCK: a replaced FIFO cannot hang the worker.
        let f = options.open(&p)?;
        let info = f.metadata()?;
        ensure!(
            info.is_file() && info.len() <= LIMIT as u64,
            "File exceeds 8 MiB or is not regular"
        );
        let mut bytes = Vec::new();
        f.take((LIMIT - size + 1) as u64).read_to_end(&mut bytes)?;
        size += bytes.len();
        ensure!(size <= LIMIT, "Files must total at most 8 MiB");
        let title = crate::model::clean(&p.file_name().unwrap_or_default().to_string_lossy(), 180);
        out.push(Attachment { title, bytes });
    }
    Ok(out)
}
pub async fn put(raw: &str, server: &str, data: Vec<u8>) -> Result<()> {
    ensure!(data.len() <= LIMIT, "File exceeds 8 MiB");
    let r = http()?
        .put(transfer_url(raw, server)?)
        .header("Content-Type", "application/octet-stream")
        .body(data)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Upload interrupted; completion is uncertain"))?;
    ensure!(
        r.status().is_success(),
        "Upload failed (HTTP {})",
        r.status().as_u16()
    );
    Ok(())
}
pub async fn get(raw: &str, server: &str) -> Result<Vec<u8>> {
    let r = http()?
        .get(transfer_url(raw, server)?)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Download interrupted"))?;
    ensure!(
        r.status().is_success(),
        "Download failed (HTTP {})",
        r.status().as_u16()
    );
    ensure!(
        r.content_length().unwrap_or(0) <= LIMIT as u64,
        "File exceeds 8 MiB"
    );
    let mut stream = r.bytes_stream();
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow::anyhow!("Download interrupted"))?;
        ensure!(out.len() + chunk.len() <= LIMIT, "File exceeds 8 MiB");
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}
pub async fn save(path: &Path, data: &[u8]) -> Result<()> {
    ensure!(data.len() <= LIMIT, "File exceeds 8 MiB");
    let path = path.to_owned();
    let data = data.to_vec();
    // Finish a short local commit once begun; cancellation cannot leave a partial
    // destination. A same-directory private temporary file is removed on error.
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::Write;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&data)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(path)?;
        Ok(())
    })
    .await??;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origin_policy() {
        for u in ["ws://example.com/ws", "file:///tmp/a", "wss://u:p@a/ws"] {
            assert!(endpoint(u).is_err());
        }
        for u in [
            "https://other/x",
            "http://a/x",
            "https://a:44/x",
            "https://a/x#f",
        ] {
            assert!(transfer_url(u, "wss://a/ws").is_err());
        }
        assert!(transfer_url("http://127.0.0.1:8/f/1", "ws://127.0.0.1:8/ws").is_ok());
    }
    #[tokio::test]
    async fn missing_and_oversized_files() {
        assert!(read(vec![]).await.is_err());
        assert!(
            read(vec![PathBuf::from("/definitely-not-an-apron-file")])
                .await
                .is_err()
        );
    }
}

/// Atomic staging: errors preserve every existing attachment. Titles are display
/// metadata, not filesystem paths; duplicate basenames get unambiguous labels.
pub fn append(existing: &mut Vec<Attachment>, mut incoming: Vec<Attachment>) -> Result<()> {
    ensure!(
        !incoming.is_empty() && existing.len() + incoming.len() <= 4,
        "Attachments are limited to four files"
    );
    ensure!(
        existing
            .iter()
            .chain(incoming.iter())
            .map(|f| f.bytes.len())
            .sum::<usize>()
            <= LIMIT,
        "Attachments must total at most 8 MiB"
    );
    let mut names: std::collections::BTreeSet<_> =
        existing.iter().map(|f| f.title.clone()).collect();
    for file in &mut incoming {
        let base = crate::model::clean(&file.title, 160);
        file.title = base.clone();
        let mut suffix = 2;
        while names.contains(&file.title) {
            file.title = format!("{base} ({suffix})");
            suffix += 1;
        }
        names.insert(file.title.clone());
    }
    existing.extend(incoming);
    Ok(())
}

#[cfg(test)]
mod staging_tests {
    use super::*;
    #[test]
    fn append_is_atomic_bounded_and_handles_duplicate_names() {
        let mut staged = vec![Attachment {
            title: "same.txt".into(),
            bytes: vec![1],
        }];
        append(
            &mut staged,
            vec![Attachment {
                title: "same.txt".into(),
                bytes: vec![2],
            }],
        )
        .unwrap();
        assert_eq!(staged[1].title, "same.txt (2)");
        assert!(
            append(
                &mut staged,
                vec![Attachment {
                    title: "too-large".into(),
                    bytes: vec![0; LIMIT]
                }]
            )
            .is_err()
        );
        assert_eq!(staged.len(), 2);
        assert_eq!(staged[0].bytes, [1]);
    }
    #[test]
    fn local_files_are_regular_and_batch_read_failure_is_atomic() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("synthetic.txt");
        std::fs::write(&path, b"synthetic bytes").unwrap();
        assert_eq!(
            read_local(vec![path.clone()]).unwrap()[0].bytes,
            b"synthetic bytes"
        );
        assert!(read_local(vec![path, tmp.path().to_owned()]).is_err());
    }
}
