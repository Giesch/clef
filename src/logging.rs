// This is a hacky setup to avoid needing to set env vars in powershell.
// Consider using a config crate or something, and switching to tracing.

const VARS: [(&str, &str); 3] = [
    ("RUST_BACKTRACE", "full"),
    ("RUST_LIB_BACKTRACE", "full"),
    ("RUST_LOG", "clef=debug"),
];

pub fn init() {
    let debug = std::env::args().nth(1).as_deref() == Some("--debug");

    if debug {
        for (k, v) in VARS {
            std::env::set_var(k, v);
        }
    } else {
        for (k, _v) in VARS {
            std::env::set_var(k, "");
        }
    }

    pretty_env_logger::init();
}
