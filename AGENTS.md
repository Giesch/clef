# Clef

## Project

Clef is a COSMIC desktop application: a native local music player.

- App ID: `io.github.giesch.clef`. The app ID is in the `justfile`, `crates/ui/src/app.rs`, and `resources/`.
- UI framework: `libcosmic` from git (`pop-os/libcosmic`), pinned to one `rev`. It is an iced fork with an Elm-style update loop.
- Platform: Linux only. Audio output uses PulseAudio.
- Rust edition: 2021.

## Build

- `just run` builds and runs the release profile.
- `just check` runs clippy with the CI settings.
- `python scripts/test_headless.py` runs the script tests and `cargo test --workspace --locked` with isolated XDG directories. Use it instead of a plain `cargo test`.
- `python scripts/check_dependencies.py` checks the direct dependencies against `scripts/dependency_snapshot.json`.

## Layout

- `src/main.rs` opens the database, starts the audio thread, and starts the UI.
- `crates/db` holds the SQLite schema, migrations, and queries (diesel).
- `crates/audio` holds the audio thread, decoding (symphonia), PulseAudio output, and MPRIS media controls (souvlaki).
- `crates/ui/src/app.rs` holds `App` (the `cosmic::Application`), the `Message` enum, the pure `update()` function, and the `view()` functions.
- `crates/ui/src/app/regression_tests.rs` holds the `iced_test` simulator tests.
- `crates/ui/src/i18n.rs` loads Fluent translations and exports the `fl!()` macro.
- `crates/ui/i18n/en/clef_ui.ftl` holds the English strings.
- All user-facing strings go through `fl!()`.

## Release

- `build.rs` runs `xdgen`. It generates `target/xdgen/app.desktop` and `target/xdgen/app.metainfo.xml` from `resources/` and the Fluent strings.
- Edit `resources/app.desktop` and `resources/app.metainfo.xml`. Do not edit the files in `target/xdgen/`.
- `scripts/release.sh` sets the version in `Cargo.toml`, writes the release notes into `resources/app.metainfo.xml`, commits, tags `v<version>`, and pushes. It runs only on `main`.
- The tag starts `.github/workflows/release.yml`. It builds a `.deb` and creates the GitHub Release.

## Packaging

- Add `.deb` runtime dependencies to the `Depends:` line in the control file in `release.yml`.
- Add build dependencies to the `apt-get install` lines in `ci.yml` and `release.yml`.
