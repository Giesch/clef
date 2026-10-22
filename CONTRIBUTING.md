# Local Development

Clef supports Windows and Linux. Use the current stable Rust toolchain, with
Rustfmt and Clippy. The dependency update was verified on 2026-10-16; this
project does not claim an independently tested minimum Rust version. Keep the
2021 edition and commit `Cargo.lock` when changing dependencies.

## Native prerequisites

On Ubuntu, install the native development libraries before building:

```sh
sudo apt-get install pkg-config cmake libpulse-dev libfontconfig1-dev libfreetype6-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libxkbcommon-dev libwayland-dev libegl1-mesa-dev
```

SQLite is bundled by `libsqlite3-sys`; no system SQLite installation is needed.
Linux audio uses PulseAudio. Windows uses CPAL and requires the native MSVC
build tools supplied by a standard Rust Windows development installation.
The CI workflow installs prerequisites only on its disposable runners.

## Reproducible validation

Run these commands from the repository root on each supported OS. The examples
use `python`; on Linux installations without that command, use `python3` instead
(Python 3.11 or newer is recommended).

```sh
python scripts/check_dependencies.py
cargo metadata --locked --format-version 1
cargo check --workspace --all-targets --locked
python scripts/test_headless.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
```

The headless script runs `cargo test --workspace --locked` with temporary
configuration, data and cache roots, removes inherited `DATABASE_URL`, and clears
desktop-session environment variables. Tests must not initialize live audio
devices, media-control services, or use the application's database or a personal
music library. Native Windows CI is required for Windows-gated platform and resampling code; Linux checks do
not establish Windows runtime compatibility. Automated UI tests use the Iced
simulator with `ICED_TEST_BACKEND=tiny-skia`, not screenshots or a GPU. The
headless script sets this CPU-renderer selection automatically; set it explicitly
when running the simulator tests directly.

The dependency audit compares direct requirements and resolutions against
`scripts/dependency_snapshot.json`. Older *transitive* packages can legitimately
coexist with these direct versions. Routine CI does not query the registry for
"latest". At upgrade time, explicitly recheck stable, non-yanked releases:

```sh
python scripts/check_dependencies.py --verify-registry
cargo update
```

Update the snapshot and requirements together if the registry has newer stable
versions, then refresh and commit the lockfile and rerun validation. Synthetic
media fixtures must document their generator, license, tags and exact expected
sample/frame counts next to the fixtures. Do not add private music recordings.

## Manual desktop smoke checks

Run these supplementary checks on both real Linux and Windows desktops; passing
headless tests does not validate hardware or desktop integration:

- Launch with an existing library and verify album navigation and artwork.
- Play, pause, skip forward/back and seek; let tracks finish and advance.
- Verify channel order and audio quality, including differently sampled files.
- Resize, hover rows and use Space; verify visible playback state and controls.
- Verify OS media keys, media metadata and artwork.
- Exit and restart; verify the library and playback configuration remain intact.

Record which platforms and devices were actually checked. No database schema or
user configuration migration is intended by the dependency upgrade.

## Optional development tools

[just](https://github.com/casey/just), [bacon](https://dystroy.org/bacon), and
[diesel_cli](https://crates.io/crates/diesel_cli) can help local development:

```sh
cargo install just
cargo install --locked bacon
cargo install diesel_cli --no-default-features --features sqlite-bundled
```

Use a disposable database when developing migrations. On Linux, `.env.example`
can be copied to `.env`; inspect its database path before running any commands.
On Windows set `DATABASE_URL` explicitly to the intended development database.
Never run test migrations against the user's Clef database.

## Linux packaging

The app ID is `io.github.giesch.clef`. The app ID is in `justfile`,
`crates/ui/src/setup.rs`, and the files in `resources/`. Do not change the app
ID after the first release.

- `resources/app.desktop` is the desktop entry.
- `resources/app.metainfo.xml` is the AppStream metainfo. `scripts/release.sh`
  writes the release notes into this file.
- `resources/icons/hicolor/` holds the app icon in 3 PNG sizes.

Validate the desktop entry and the metainfo after each change:

```sh
desktop-file-validate resources/app.desktop
appstreamcli validate --no-net resources/app.metainfo.xml
```

Install into a staging root and remove the files again:

```sh
just build-release
just rootdir=/tmp/clef-root install
just rootdir=/tmp/clef-root uninstall
```

Without `rootdir`, `install` writes to `/usr` and needs root access.

### Releases

Run `just release` on `main` with a clean working tree. The script sets the
version in `Cargo.toml`, adds the release notes to the metainfo, commits, tags
`v<version>`, and pushes.

The tag starts `.github/workflows/release.yml`. This workflow builds a `.deb`
and creates the GitHub Release.
