use std::fs;
use std::path::Path;

use xdgen::{App, Context, FluentString};

const I18N_DIR: &str = "crates/ui/i18n";
const DESKTOP_TEMPLATE: &str = "resources/app.desktop";
const METAINFO_TEMPLATE: &str = "resources/app.metainfo.xml";

fn main() {
    println!("cargo:rerun-if-changed={I18N_DIR}");
    println!("cargo:rerun-if-changed={DESKTOP_TEMPLATE}");
    println!("cargo:rerun-if-changed={METAINFO_TEMPLATE}");

    let ctx = Context::new(I18N_DIR, "clef_ui").unwrap();
    let app = App::new(FluentString("app-title"))
        .comment(FluentString("app-comment"))
        .keywords(FluentString("app-keywords"));

    let desktop_entry = app.expand_desktop(DESKTOP_TEMPLATE, &ctx).unwrap();
    let metainfo = app.expand_metainfo(METAINFO_TEMPLATE, &ctx).unwrap();

    // Packaging reads these paths, so they do not follow CARGO_TARGET_DIR.
    let output = Path::new("target/xdgen");
    fs::create_dir_all(output).unwrap();
    fs::write(output.join("app.desktop"), desktop_entry).unwrap();
    fs::write(output.join("app.metainfo.xml"), metainfo).unwrap();
}
