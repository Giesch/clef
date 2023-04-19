dev:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    RUST_LIB_BACKTRACE=full RUST_BACKTRACE=full RUST_LOG=clef=info \
    cargo run

run:
    __NV_PRIME_RENDER_OFFLOAD=1 \
    cargo run --release

reset: remove-db remove-cache

remove-db:
    rm -rf $HOME/.local/share/clef/db.sqlite*

remove-cache:
    rm -rf $HOME/.local/share/clef/resized_images

# NOTE re: __NV_PRIME_RENDER_OFFLOAD=1
# this is specific to my machine;
# it asks pop os to use the gpu in hybrid mode
# https://support.system76.com/articles/graphics-switch-pop/
