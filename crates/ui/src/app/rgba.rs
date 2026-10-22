use camino::{Utf8Path, Utf8PathBuf};
use cosmic::iced::widget::image;
use cosmic::iced::widget::image::Handle;
use image_rs::Rgba;
use image_rs::{imageops::FilterType, ColorType, ImageBuffer};

/// The size that album art gets resized (down) to.
pub const IMAGE_SIZE: u16 = 256;

/// Image pixels in the format that iced converts them to internally
/// Doing the conversion ahead of time (outside the framework)
/// makes it possible to do that work off the ui thread.
/// The slow operations to avoid include both the resize and the format conversion.
///
/// https://github.com/iced-rs/iced/issues/549
#[derive(Clone, Debug)]
pub struct RgbaBytes {
    handle: Handle,
}

impl RgbaBytes {
    #[cfg(test)]
    pub fn empty() -> Self {
        let handle = Handle::from_rgba(0, 0, vec![]);
        Self { handle }
    }

    fn from_buffer(rgba: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Self {
        let width = rgba.width();
        let height = rgba.height();
        let bytes = rgba.into_raw();
        let handle = Handle::from_rgba(width, height, bytes);

        RgbaBytes { handle }
    }
}

impl From<&RgbaBytes> for image::Handle {
    fn from(rgba_bytes: &RgbaBytes) -> Self {
        // NOTE The handle uses an Arc internally, so this clone is cheap
        rgba_bytes.handle.clone()
    }
}

// NOTE this is slow
pub fn load_rgba(path: &Utf8PathBuf) -> anyhow::Result<RgbaBytes> {
    let img = image_rs::open(path)?;
    let img = img.resize(
        u32::from(IMAGE_SIZE),
        u32::from(IMAGE_SIZE),
        FilterType::Lanczos3,
    );

    let rgba_bytes = RgbaBytes::from_buffer(img.to_rgba8());

    Ok(rgba_bytes)
}

// NOTE this assumes that the 'conversion' to rgba8
// will be fast because it's already in the right format
pub fn load_cached_rgba_bmp(path: &Utf8Path) -> anyhow::Result<RgbaBytes> {
    let img = image_rs::open(path)?;
    let rgba_bytes = RgbaBytes::from_buffer(img.to_rgba8());

    Ok(rgba_bytes)
}

pub fn save_rgba(path: &Utf8PathBuf, rgba: &RgbaBytes) -> anyhow::Result<()> {
    let RgbaBytes { handle } = rgba;
    let (pixels, width, height) = match handle {
        Handle::Rgba { pixels, width, height, .. } => (pixels, width, height),
        _ => unreachable!("RgbaBytes always contains decoded RGBA pixels"),
    };

    image_rs::save_buffer(path, pixels, *width, *height, ColorType::Rgba8)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_image_roundtrip() {
        let pixels = vec![255, 0, 0, 128, 0, 255, 0, 0];
        let original =
            RgbaBytes::from_buffer(ImageBuffer::from_raw(2, 1, pixels.clone()).unwrap());
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = loop {
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let directory = std::env::temp_dir()
                .join(format!("clef-rgba-{}-{sequence}", std::process::id()));
            match std::fs::create_dir(&directory) {
                Ok(()) => break Directory(directory),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    continue
                }
                Err(error) => panic!("create isolated image directory: {error}"),
            }
        };
        let path = Utf8PathBuf::from_path_buf(directory.0.join("art.bmp")).unwrap();
        save_rgba(&path, &original).unwrap();
        let loaded = load_cached_rgba_bmp(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        match loaded.handle {
            Handle::Rgba { width, height, pixels: loaded, .. } => {
                assert_eq!((width, height), (2, 1));
                assert_eq!(loaded.as_ref(), pixels.as_slice());
            }
            _ => panic!("expected decoded RGBA handle"),
        }
    }
}
