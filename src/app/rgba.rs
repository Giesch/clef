use camino::{Utf8Path, Utf8PathBuf};
use iced::widget::image;
use iced_native::image::Handle;
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
#[derive(Clone)]
pub struct RgbaBytes {
    width: u32,
    height: u32,
    handle: Handle,
}

impl RgbaBytes {
    #[cfg(test)]
    pub fn empty() -> Self {
        let handle = Handle::from_pixels(0, 0, vec![]);
        Self { width: 0, height: 0, handle }
    }

    fn from_buffer(rgba: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Self {
        let width = rgba.width();
        let height = rgba.height();
        let bytes = rgba.into_raw();
        let handle = Handle::from_pixels(width, height, bytes);

        RgbaBytes { width, height, handle }
    }
}

impl From<&RgbaBytes> for image::Handle {
    fn from(rgba_bytes: &RgbaBytes) -> Self {
        // NOTE The handle uses an Arc internally, so this clone is cheap
        rgba_bytes.handle.clone()
    }
}

impl std::fmt::Debug for RgbaBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RgbaBytes")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
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
    use iced_native::image::Data;

    let RgbaBytes { width, height, handle } = rgba;
    let bytes = match handle.data() {
        Data::Path(_) => unreachable!(),
        Data::Bytes(bytes) => bytes,
        Data::Rgba { pixels, .. } => pixels,
    };

    image_rs::save_buffer(path, bytes, *width, *height, ColorType::Rgba8)?;

    Ok(())
}
