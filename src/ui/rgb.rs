use camino::Utf8PathBuf;
use iced::widget::image;

/// Image pixels in the format that iced converts them to internally
/// Doing the conversion ahead of time (outside the framework)
/// makes it possible to do it off the ui thread
///
/// https://github.com/iced-rs/iced/issues/549
#[derive(Clone)]
pub struct RgbBytes {
    height: u32,
    width: u32,
    bytes: Vec<u8>,
}

impl From<RgbBytes> for image::Handle {
    fn from(bgra_bytes: RgbBytes) -> Self {
        image::Handle::from_rgba(bgra_bytes.width, bgra_bytes.height, bgra_bytes.bytes)
    }
}

impl std::fmt::Debug for RgbBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BgraBytes")
            .field("height", &self.height)
            .field("width", &self.width)
            .finish()
    }
}

/// NOTE this is slow
pub fn load_bgra(path: &Utf8PathBuf) -> Option<RgbBytes> {
    let img = image_rs::open(path).ok()?;
    let rgb = img.to_rgb8();

    let rgb_bytes = RgbBytes {
        height: rgb.height(),
        width: rgb.width(),
        bytes: rgb.into_raw(),
    };

    Some(rgb_bytes)
}
