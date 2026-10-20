# Audio fixtures

`tone.webm` is an original 440 Hz sine wave at gain 0.25, recorded for approximately
300 ms using Chromium MediaRecorder with WebM/Opus. It contains no third-party media.
The portable assertion suite embeds its bytes so it also runs without filesystem access.

The suite generates its four-frame PCM WAV and six-frame MPEG silence fixtures from
fixed test bytes. File extensions do not participate in decoder selection.
