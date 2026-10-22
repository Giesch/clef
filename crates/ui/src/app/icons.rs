use cosmic::widget::icon::{self, Icon};

pub fn play() -> Icon {
    symbolic(include_bytes!("../../resources/play.svg"))
}
pub fn pause() -> Icon {
    symbolic(include_bytes!("../../resources/pause.svg"))
}
pub fn forward() -> Icon {
    symbolic(include_bytes!("../../resources/skip-forward.svg"))
}
pub fn back() -> Icon {
    symbolic(include_bytes!("../../resources/skip-back.svg"))
}

fn symbolic(bytes: &'static [u8]) -> Icon {
    let mut handle = icon::from_svg_bytes(bytes);
    handle.symbolic = true;

    icon::icon(handle).size(24)
}
