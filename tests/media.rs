use apron_gpui::media::{self, LIMIT};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
async fn response(
    status: &str,
    headers: &str,
    body: &str,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!("HTTP/1.1 {status}\r\n{headers}\r\nConnection: close\r\n\r\n{body}");
    let task = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut buf = vec![0; 4096];
        let n = s.read(&mut buf).await.unwrap();
        let _ = s.write_all(response.as_bytes()).await;
        String::from_utf8_lossy(&buf[..n]).into_owned()
    });
    (format!("127.0.0.1:{}", address.port()), task)
}
#[tokio::test]
async fn rejects_redirects_and_oversized_headers_without_forwarding_auth() {
    let (a, request) = response(
        "302 Found",
        "Location: https://example.invalid/secret\r\nContent-Length: 0",
        "",
    )
    .await;
    assert!(
        media::get(&format!("http://{a}/file"), &format!("ws://{a}/ws"))
            .await
            .unwrap_err()
            .to_string()
            .contains("302")
    );
    let r = request.await.unwrap().to_lowercase();
    assert!(!r.contains("authorization:"));
    assert!(!r.contains("cookie:"));
    let (a, request) = response("200 OK", &format!("Content-Length: {}", LIMIT + 1), "").await;
    assert!(
        media::get(&format!("http://{a}/file"), &format!("ws://{a}/ws"))
            .await
            .is_err()
    );
    request.await.unwrap();
}
#[tokio::test]
async fn actual_file_bounds_and_no_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let large = dir.path().join("large");
    let f = std::fs::File::create(&large).unwrap();
    f.set_len(LIMIT as u64 + 1).unwrap();
    assert!(media::read(vec![large]).await.is_err());
    assert!(media::read(vec![dir.path().to_owned()]).await.is_err());
    let saved = dir.path().join("saved");
    media::save(&saved, b"synthetic").await.unwrap();
    assert!(media::save(&saved, b"overwrite").await.is_err());
    assert_eq!(std::fs::read(saved).unwrap(), b"synthetic");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn chunked_stream_cannot_bypass_byte_limit() {
    let bytes = LIMIT + 1;
    let body = format!("{:x}\r\n{}\r\n0\r\n\r\n", bytes, "x".repeat(bytes));
    let (address, request) = response("200 OK", "Transfer-Encoding: chunked", &body).await;
    let result = media::get(
        &format!("http://{address}/file"),
        &format!("ws://{address}/ws"),
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("8 MiB"));
    request.await.unwrap();
}
