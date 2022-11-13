use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use directories::{ProjectDirs, UserDirs};

pub fn local_data_dir() -> anyhow::Result<Utf8PathBuf> {
    let project_dirs =
        project_dirs().context("no project directory path for app found")?;
    let local_data = project_dirs.data_local_dir();
    let local_data: &Utf8Path = local_data
        .try_into()
        .context("non-utf8 local data directory")?;

    Ok(local_data.to_owned())
}

pub fn db_path() -> anyhow::Result<Utf8PathBuf> {
    let project_dirs =
        project_dirs().context("no project directory path for app found")?;
    let db_path = project_dirs.data_local_dir().join("db.sqlite");
    let db_path: Utf8PathBuf = db_path
        .try_into()
        .context("non-utf8 local data directory")?;

    Ok(db_path)
}

pub fn audio_dir() -> anyhow::Result<Utf8PathBuf> {
    let user_dirs = UserDirs::new().context("no user directories")?;
    let audio_dir = user_dirs.audio_dir().context("no audio directory")?;
    let audio_dir: &Utf8Path = audio_dir.try_into().context("invalid utf8")?;

    Ok(audio_dir.to_owned())
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "Clef")
}
