dev:
    RUST_LIB_BACKTRACE=full RUST_BACKTRACE=full RUST_LOG=clef=info cargo run

check:
    cargo watch -q -c -x check

test:
    cargo watch -q -c -x test

lint:
    cargo watch -q -c -x clippy

remove-db:
    rm -rf $HOME/.local/share/clef/db.sqlite*

remove-cached-images:
    rm -rf "$HOME/.local/share/clef/resized_images"
