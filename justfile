set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# Name of the application's binary.
name := 'clef'
# The unique ID of the application.
appid := 'io.github.giesch.clef'

# Path to root file system, which defaults to `/`.
rootdir := ''
# The prefix for the `/usr` directory.
prefix := '/usr'
# The location of the cargo target directory.
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')

# Install destinations
base-dir := absolute_path(clean(rootdir / prefix))
bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share' / 'applications' / appid + '.desktop'
metainfo-dst := base-dir / 'share' / 'metainfo' / appid + '.metainfo.xml'
icons-dst := base-dir / 'share' / 'icons' / 'hicolor'
icon-sizes := '64x64 128x128 256x256'

# list available commands
list:
    just -l

# run with debug logs and backtraces
[linux]
dev:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    cargo run -- --debug

# run with debug logs and backtraces
[windows]
dev:
    cargo run -- --debug

# run all tests once
test:
    cargo test --all

# run all tests continuously
test-watch:
    bacon test -- --all

# run in release mode
[linux]
run:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    cargo run --release

# run in release mode
[windows]
run:
    cargo run --release

# compile with the debug profile
build-debug *args:
    cargo build --locked {{args}}

# compile with the release profile
build-release *args: (build-debug '--release' args)

# run clippy with the CI settings
check *args:
    cargo clippy --workspace --all-targets --locked {{args}}

# run clippy with JSON messages
check-json: (check '--message-format=json')

# install the release binary, desktop entry, metainfo, and icons
[linux]
install:
    install -Dm0755 {{ cargo-target-dir / 'release' / name }} {{bin-dst}}
    install -Dm0644 resources/app.desktop {{desktop-dst}}
    install -Dm0644 resources/app.metainfo.xml {{metainfo-dst}}
    for size in {{icon-sizes}}; do \
        install -Dm0644 "resources/icons/hicolor/$size/apps/{{appid}}.png" "{{icons-dst}}/$size/apps/{{appid}}.png"; \
    done

# remove the files that 'install' copies
[linux]
uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{metainfo-dst}}
    for size in {{icon-sizes}}; do rm -f "{{icons-dst}}/$size/apps/{{appid}}.png"; done

# run cargo clean
clean:
    cargo clean

# set the version, tag v<version>, and push a release
[linux]
release:
    scripts/release.sh

# delete the database and image cache
reset: remove-db remove-cache

# delete the sqlite database
[linux]
remove-db:
    rm $HOME/.local/share/clef/db.sqlite*

# delete the sqlite database
[windows]
remove-db:
    rm $HOME\AppData\Local\Clef\data\db.sqlite*

# delete the cache of resized images
[linux]
remove-cache:
    rm -rf $HOME/.local/share/clef/resized_images

# delete the cache of resized images
[windows]
remove-cache:
    rm $HOME\AppData\Local\Clef\data\resized_images\*

# NOTE
# 'unused' depends on cargo-udeps and rust nightly:
# cargo install cargo-udeps --locked
# rustup install nightly

# check for unused dependencies
unused:
    cargo +nightly udeps --all-targets --workspace

# NOTE re: __NV_PRIME_RENDER_OFFLOAD=1
# this is specific to my machine;
# it ensures pop os uses the gpu while in hybrid mode
# https://support.system76.com/articles/graphics-switch-pop/
