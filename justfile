set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

[linux]
dev:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    cargo run -- --debug

[windows]
dev:
    cargo run -- --debug

[linux]
run:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    cargo run --release

[windows]
run:
    cargo run --release

reset: remove-db remove-cache

[linux]
remove-db:
    rm $HOME/.local/share/clef/db.sqlite*

[linux]
remove-cache:
    rm -rf $HOME/.local/share/clef/resized_images

[windows]
remove-db:
    rm $HOME\AppData\Local\Clef\data\db.sqlite*

[windows]
remove-cache:
    rm -r -fo $HOME\AppData\Local\Clef\data\resized_images

# NOTE re: __NV_PRIME_RENDER_OFFLOAD=1
# this is specific to my machine;
# it ensures pop os uses the gpu while in hybrid mode
# https://support.system76.com/articles/graphics-switch-pop/
