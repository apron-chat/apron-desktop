//! Bounded raster decoding. No SVG, URLs, animations or metadata are passed to GPUI.
use anyhow::{Result, ensure};
use image::{ImageDecoder, ImageFormat, ImageReader};
use std::io::Cursor;
pub const MAX_SIDE: u32 = 4096;
pub const MAX_PIXELS: u64 = 8_000_000;
#[derive(Debug)]
pub struct Raster {
    pub png: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
pub fn normalize(bytes: &[u8]) -> Result<Raster> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= crate::media::LIMIT,
        "Image must be at most 8 MiB"
    );
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    ensure!(
        matches!(
            reader.format(),
            Some(
                ImageFormat::Png
                    | ImageFormat::Jpeg
                    | ImageFormat::WebP
                    | ImageFormat::Gif
                    | ImageFormat::Bmp
            )
        ),
        "Use a PNG, JPEG, WebP, GIF or BMP image"
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    ensure!(
        w > 0
            && h > 0
            && w <= MAX_SIDE
            && h <= MAX_SIDE
            && u64::from(w) * u64::from(h) <= MAX_PIXELS,
        "Image exceeds 4096 pixels per side or 8 million pixels"
    );
    let orientation = decoder.orientation()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orientation);
    let width = decoded.width();
    let height = decoded.height();
    let mut png = Cursor::new(Vec::new());
    decoded.write_to(&mut png, ImageFormat::Png)?;
    ensure!(
        png.get_ref().len() <= crate::media::LIMIT,
        "Converted image exceeds 8 MiB"
    );
    let mut thumbnail = Cursor::new(Vec::new());
    decoded
        .thumbnail(640, 480)
        .write_to(&mut thumbnail, ImageFormat::Png)?;
    Ok(Raster {
        png: png.into_inner(),
        thumbnail: thumbnail.into_inner(),
        width,
        height,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_rasters_and_rejects_oversize_or_active_formats() {
        let image = image::DynamicImage::new_rgb8(40, 24);
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::Bmp,
            ImageFormat::WebP,
        ] {
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            let normalized = normalize(bytes.get_ref()).unwrap();
            assert_eq!((normalized.width, normalized.height), (40, 24));
            assert!(normalized.png.starts_with(b"\x89PNG"));
        }
        assert!(
            normalize(
                b"<svg xmlns='http://www.w3.org/2000/svg'><image href='https://invalid'/></svg>"
            )
            .is_err()
        );
        assert!(normalize(&vec![0; crate::media::LIMIT + 1]).is_err());
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4097, 1)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        assert!(normalize(bytes.get_ref()).is_err());
    }
}
