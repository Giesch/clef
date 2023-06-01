set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# run with debug logs and backtraces
[linux]
dev:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    cargo run -- --debug

# run with debug logs and backtraces
[windows]
dev:
    cargo run -- --debug

test:
    cargo test --all

# run in release mode
[linux]
run:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    cargo run --release

# run in release mode
[windows]
run:
    cargo run --release

# delete the database and image cache
reset: remove-db remove-cache

# delete the sqlite database
[linux]
remove-db:
    rm $HOME/.local/share/clef/db.sqlite*

# clear the cache of resized images
[linux]
remove-cache:
    rm -rf $HOME/.local/share/clef/resized_images

# clear the cache of resized images
[windows]
remove-db:
    rm $HOME\AppData\Local\Clef\data\db.sqlite*

# clear the cache of resized images
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
