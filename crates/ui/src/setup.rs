use crate::app::{App, Flags};

pub const APP_ID: &str = "io.github.giesch.clef";

pub fn launch(flags: Flags) -> anyhow::Result<()> {
    iced::application(move || App::boot(flags.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(window_settings())
        .run()?;

    Ok(())
}

fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        icon: crate::icon::get_icon(),
        #[cfg(target_os = "linux")]
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: APP_ID.to_string(),
            ..Default::default()
        },
        ..Default::default()
    }
}
