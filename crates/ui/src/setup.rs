use crate::app::{App, Flags};

pub fn launch(flags: Flags) -> anyhow::Result<()> {
    iced::application(move || App::boot(flags.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(iced::window::Settings {
            icon: crate::icon::get_icon(),
            ..Default::default()
        })
        .run()?;

    Ok(())
}
