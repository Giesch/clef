use directories::ProjectDirs;

pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "Clef")
}
