use camino::Utf8PathBuf;
use iced::widget::image;
use image_rs::{imageops::FilterType, ColorType};

/// Image pixels in the format that iced converts them to internally
/// Doing the conversion ahead of time (outside the framework)
/// makes it possible to do it off the ui thread
///
/// https://github.com/iced-rs/iced/issues/549
#[derive(Clone)]
pub struct RgbaBytes {
    height: u32,
    width: u32,
    bytes: Vec<u8>,
}

impl From<RgbaBytes> for image::Handle {
    fn from(rgba_bytes: RgbaBytes) -> Self {
        image::Handle::from_pixels(rgba_bytes.width, rgba_bytes.height, rgba_bytes.bytes)
    }
}

impl std::fmt::Debug for RgbaBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RgbaBytes")
            .field("height", &self.height)
            .field("width", &self.width)
            .finish()
    }
}

pub const IMAGE_SIZE: u16 = 256;

/// NOTE this is slow
pub fn load_rgba(path: &Utf8PathBuf) -> Option<RgbaBytes> {
    let img = image_rs::open(path).ok()?;
    let img = img.resize(IMAGE_SIZE as u32, IMAGE_SIZE as u32, FilterType::Lanczos3);

    let rgba = img.to_rgba8();

    let rgba_bytes = RgbaBytes {
        height: rgba.height(),
        width: rgba.width(),
        bytes: rgba.into_raw(),
    };

    Some(rgba_bytes)
}

pub fn save_rgba(path: &Utf8PathBuf, rgba: &RgbaBytes) -> anyhow::Result<()> {
    let RgbaBytes { height, width, bytes } = rgba;
    image_rs::save_buffer(path, bytes, *width, *height, ColorType::Rgba8)?;

    Ok(())
}
