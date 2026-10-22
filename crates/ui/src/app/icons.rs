use iced::widget::{svg, Svg};

pub fn play() -> Svg<'static> {
    svg_icon("play.svg")
}
pub fn pause() -> Svg<'static> {
    svg_icon("pause.svg")
}
pub fn forward() -> Svg<'static> {
    svg_icon("skip-forward.svg")
}
pub fn back() -> Svg<'static> {
    svg_icon("skip-back.svg")
}

fn svg_icon(file_name: &str) -> Svg<'static> {
    let root = env!("CARGO_MANIFEST_DIR");
    svg(svg::Handle::from_path(format!(
        "{root}/resources/{file_name}"
    )))
}
