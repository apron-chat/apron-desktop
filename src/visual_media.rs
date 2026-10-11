//! Explicit raster sources. No redirects, foreign origins or active image formats.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
pub const AVATAR_SOURCE_LIMIT: usize = 65_536;
pub fn inline_avatar(source: &str) -> Result<Option<Vec<u8>>> {
    ensure!(
        source.len() <= AVATAR_SOURCE_LIMIT,
        "Avatar address exceeds 64 KiB"
    );
    if !source.starts_with("data:") {
        return Ok(None);
    }
    let (header, encoded) = source
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("Invalid inline avatar"))?;
    ensure!(
        matches!(
            header,
            "data:image/png;base64"
                | "data:image/jpeg;base64"
                | "data:image/webp;base64"
                | "data:image/gif;base64"
        ),
        "Inline avatar must be PNG, JPEG, WebP or GIF"
    );
    let bytes = STANDARD.decode(encoded)?;
    ensure!(
        matches!(
            image::guess_format(&bytes)?,
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::WebP
                | image::ImageFormat::Gif
        ),
        "Unsupported avatar raster"
    );
    Ok(Some(bytes))
}
pub fn allowed(source: &str, server: &str, avatar: bool) -> Result<()> {
    if avatar && inline_avatar(source)?.is_some() {
        return Ok(());
    }
    crate::media::transfer_url(source, server)?;
    Ok(())
}
pub async fn load(source: &str, server: &str, avatar: bool) -> Result<crate::images::Raster> {
    allowed(source, server, avatar)?;
    let bytes = if avatar { inline_avatar(source)? } else { None };
    let bytes = match bytes {
        Some(bytes) => bytes,
        None => crate::media::get(source, server).await?,
    };
    tokio::task::spawn_blocking(move || {
        let mut raster = crate::images::normalize(&bytes)?;
        if avatar {
            let image = image::load_from_memory(&raster.thumbnail)?.thumbnail(128, 128);
            let mut output = std::io::Cursor::new(Vec::new());
            image.write_to(&mut output, image::ImageFormat::Png)?;
            raster.thumbnail = output.into_inner();
        }
        Ok(raster)
    })
    .await?
}
pub fn scale(current: f32, factor: f32) -> f32 {
    (current * factor).clamp(0.1, 4.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_policy_and_zoom_are_bounded() {
        let server = "wss://chat.example/ws";
        for bad in [
            "https://other.example/a.png",
            "file:///tmp/a.png",
            "data:image/svg+xml;base64,AAAA",
            "data:image/png;base64,!!!",
            "https://u:p@chat.example/a",
        ] {
            assert!(allowed(bad, server, true).is_err());
        }
        assert!(allowed("https://chat.example/a.png", server, true).is_ok());
        assert!(inline_avatar(&"a".repeat(AVATAR_SOURCE_LIMIT + 1)).is_err());
        assert_eq!(scale(1., 100.), 4.);
        assert_eq!(scale(1., 0.001), 0.1);
    }
    #[tokio::test]
    async fn inline_avatar_normalizes_and_shrinks() {
        let mut data = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(256, 192)
            .write_to(&mut data, image::ImageFormat::Png)
            .unwrap();
        let source = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(data.into_inner())
        );
        let raster = load(&source, "wss://chat.example/ws", true).await.unwrap();
        assert_eq!((raster.width, raster.height), (256, 192));
        let thumb = image::load_from_memory(&raster.thumbnail).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (128, 96));
        assert!(allowed(&source, "wss://chat.example/ws", false).is_err());
    }
}
