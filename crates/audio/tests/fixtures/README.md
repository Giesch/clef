# Synthetic audio regression fixtures

All fixtures are mathematically generated tones or solid-color graphics, not recordings
or third-party music. They are dedicated to the public domain under CC0-1.0.
No attribution is required. No real artist or album is represented.

Generated with FFmpeg 6.1.1-3ubuntu5 (libavcodec 60.31.102, libavformat 60.16.100),
using its libmp3lame encoder. From the repository root:

```sh
ffmpeg -v error -f lavfi -i 'aevalsrc=0.25*sin(2*PI*440*t)|0.125*sin(2*PI*880*t):s=44100:d=1' -map_metadata -1 -c:a pcm_s16le -fflags +bitexact -flags:a +bitexact -y crates/audio/tests/fixtures/stereo.wav
ffmpeg -v error -f lavfi -i 'aevalsrc=0.25*sin(2*PI*440*t)|0.125*sin(2*PI*880*t):s=44100:d=4' -map_metadata -1 -c:a pcm_s16le -fflags +bitexact -flags:a +bitexact -y crates/audio/tests/fixtures/long-stereo.wav
ffmpeg -v error -f lavfi -i color=c=blue:s=16x16 -frames:v 1 -threads 1 -y crates/audio/tests/fixtures/cover.png
ffmpeg -v error -i crates/audio/tests/fixtures/stereo.wav -i crates/audio/tests/fixtures/cover.png -map 0:a -map 1:v -map_metadata -1 -metadata title='Synthetic stereo' -metadata artist='Clef tests' -metadata album='Regression fixtures' -metadata track='3/7' -metadata:s:v title='Album cover' -metadata:s:v comment='Cover (front)' -c:a libmp3lame -b:a 128k -c:v copy -write_xing 1 -id3v2_version 3 -fflags +bitexact -flags:a +bitexact -y crates/audio/tests/fixtures/tagged.mp3
```

Both WAV files are signed 16-bit PCM at 44,100 Hz, with left amplitude 0.25 at
440 Hz and right amplitude 0.125 at 880 Hz. `stereo.wav` has exactly 44,100 frames;
`long-stereo.wav` has 176,400 frames and crosses the three-second preload boundary.

`tagged.mp3` encodes the one-second signal with Xing/LAME gapless information.
FFprobe reports 1,105 initial skipped samples and 875 final discarded padding frames:
40 encoded MPEG audio frames × 1,152 samples − 1,105 − 875 = 44,100 decoded frames.
These skip counts include decoder delay; they are the effective skip/padding values,
not a claim about the encoder-only delay field. Inspect with:

```sh
ffprobe -v error -select_streams a -show_packets -show_entries packet=side_data_list -of json crates/audio/tests/fixtures/tagged.mp3
```

Tests assert the exact trimmed frame count, stereo values, title/artist/album,
track number 3, total tracks 7, and embedded PNG bytes. The longer WAV tests also
exercise cached packets followed by continued decoding and a two-track queue handoff.
Encoder versions can change MP3 bytes; regeneration must retain these decoded contracts.

Tests read repository fixtures or in-memory bytes and use a recording output. They
never initialize audio hardware, DBus controls, a database, or user music directories.
The MP3 frame-count test verifies trimmed decoding, not uninterrupted device-level
playback across output stream closure/reopening.
