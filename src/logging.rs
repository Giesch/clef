// This is a hacky setup to avoid using env vars in powershell
// consider using a config crate or something, and switching to tracing

const VARS: [(&str, &str); 3] = [
    ("RUST_BACKTRACE", "full"),
    ("RUST_LIB_BACKTRACE", "full"),
    ("RUST_LOG", "clef=trace"),
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
