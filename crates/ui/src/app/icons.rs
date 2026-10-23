use iced::widget::{svg, Svg};

pub fn play() -> Svg<'static> {
    svg_icon(include_bytes!("../../resources/play.svg"))
}
pub fn pause() -> Svg<'static> {
    svg_icon(include_bytes!("../../resources/pause.svg"))
}
pub fn forward() -> Svg<'static> {
    svg_icon(include_bytes!("../../resources/skip-forward.svg"))
}
pub fn back() -> Svg<'static> {
    svg_icon(include_bytes!("../../resources/skip-back.svg"))
}

fn svg_icon(bytes: &'static [u8]) -> Svg<'static> {
    svg(svg::Handle::from_memory(bytes))
}
