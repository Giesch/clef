# Headless resampler source check

This standalone workspace builds the production `src/resampler.rs` directly on Linux,
without changing the audio crate's platform dependency gates. It lets a Linux developer
run the same resampler regression that is normally compiled on Windows/non-Linux:

```sh
cargo test --manifest-path crates/audio/tests/resampler-harness/Cargo.toml --locked --target-dir target/audio-resampler-check
```

The test covers irregular blocks, zero/short inputs, up/downsampling, independent stereo
amplitudes, exact floor-rounded frame counts, and idempotent flush. It does not compile
CPAL's Windows backend or validate a live audio device. The production Linux audio tests
separately exercise shared callback drain accounting without audio hardware.
