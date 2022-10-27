use iced::widget::{svg, Svg};

pub fn play() -> Svg {
    svg_icon("play.svg")
}

pub fn pause() -> Svg {
    svg_icon("pause.svg")
}

fn svg_icon(file_name: &str) -> Svg {
    let project_root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{project_root}/resources/{file_name}");

    svg(svg::Handle::from_path(path))
}
