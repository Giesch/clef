use iced::{Application, Settings};

use crate::app::{App, Flags};

pub fn launch(flags: Flags) -> anyhow::Result<()> {
    let mut settings = Settings::with_flags(flags);

    settings.window = iced::window::Settings {
        icon: crate::icon::get_icon(),
        ..Default::default()
    };

    App::run(settings)?;

    Ok(())
}
