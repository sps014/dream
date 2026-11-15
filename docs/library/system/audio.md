---
title: Audio playback and FFT
summary: Play audio, select outputs, and inspect PCM and frequency data.
api: reference/api/system-audio.md
---

# Audio playback and FFT

Import `system.audio` to play WAV, MP3, and WebM/Opus on native desktops and in
browsers.

- Format detection uses the bytes. The file extension does not have to match.
- WebM uses the first Opus track and ignores video. Mono and stereo are supported.

## Play a file

```dream
import system;
import system.audio;

async fun main(): void {
    let audio = Audio("music.mp3");
    audio.volume = 0.5f;
    let started = audio.play().await;
    if started.is_err() {
        System.println(started.unwrap_err().message());
        audio.close();
        return;
    }

    while !audio.ended && audio.error.is_none() {
        for (let event in audio.poll_events()) {
            switch (event) {
                AudioEvent.Error(_, error, operation, _) => {
                    System.println(operation + ": " + error.message());
                }
                _ => {}
            }
        }
        Time.delay(16).await;
    }
    audio.close();
}
```

`Audio()` creates an empty player. Assign `src` before playing.

- Constructors do no file I/O and do not open a device.
- `Audio(bytes)` copies encoded bytes. `Audio(buffer)` uses a decoded `AudioBuffer`.
- `load().await` prepares a source without playing it. `play().await` prepares it if needed and returns when output starts.
- Node and standalone WASI can decode buffers and run FFT. They cannot play to a speaker.

`pause()` keeps the position. `play().await` resumes from there.

- Set `current_time` in seconds to seek. Streamed sources follow the rules under URLs and limits.
- `duration` is absent until loading finishes. `volume` is 0 to 1, and `muted` keeps that value.
- `loop` repeats a loaded source. `close()` is final and safe to call again.
- Dropping a player cancels loading and stops output. Keep it alive while it should play.

## URLs and limits

String sources also accept `http` and `https` URLs.

- Native and Node use Dream's HTTP client. A non-success status is `EHTTP`.
- Browser URLs use the page's media element for loading, decoding, buffering, and seeking. They follow media permissions and CORS, and a failed load is `EIO`.
- Replacing `src`, calling `load` again, or `close()` cancels the previous load. That load cannot replace the new source.

Native file and HTTP sources stream. Playback starts after the first 256 KiB arrives (or the whole file, if smaller) and a quarter second is decoded, and memory does not grow with the track.

- `load()` reads the format, the channel count, and the duration when the container declares one.
- Streamed MP3, and WebM without a declared duration, leave `duration` absent. Playback and `Ended` still work.
- Looping needs a seekable source.

If data arrives slower than playback, output goes silent, `Waiting` is reported, and `Playing` follows when data resumes.

- Seeking a local file reopens it and decodes up to the target.
- Native and Node HTTP seeking supports PCM and IEEE-float WAV. The server must advertise byte ranges, a strong ETag or Last-Modified, and Content-Length, and return that range of the same resource.
- Unsupported formats or servers return `ENOTSUP`. A bad range response returns `ESEEK` and leaves the play position unchanged.
- A remote file is not fully downloaded just to make it seekable. Browser seeking uses the media element's ranges, and supported formats depend on the browser.
- A seek stays in progress until `Seeked`. Setting `current_time` again before then returns `EBUSY`, and a target past a known duration returns `EINVAL`.

The browser buffers URL media on its own, separate from `AudioOptions`. Byte sources, local files, and `AudioBuffer.decode` use the built-in decoders.

Decoding runs off your program's thread, so timers and input keep running. A stream decodes the next chunk while the current one plays.

```dream
import system.audio;

let options = AudioOptions();
options.max_compressed_bytes = 16 * 1024 * 1024;
options.max_decoded_bytes = 64 * 1024 * 1024;
let audio = Audio("https://example.com/song.webm", options);
```

For a stream, `max_compressed_bytes` (default 8 MiB) limits encoded data held while decoding, not the track length. A header or WebM element that does not fit returns `ELIMIT`.

- `buffer_seconds` (default 1, at most 30) is how much decoded audio is queued ahead of the speaker.
- `max_decoded_bytes` (default 256 MiB) applies to byte sources, which are decoded in full. A larger result returns `ELIMIT`.
- The constructor copies the limits. Later changes to the options object do not apply.

## Receive events

`poll_events()` drains a typed queue. Timestamps are monotonic nanoseconds.

- `Play` means the start was accepted. `Playing` means output is running.
- `Seeking` and `Seeked` surround an accepted seek.
- `Pause`, `Ended`, `VolumeChange`, `SinkChange`, and `Error` report those changes.
- State properties stay readable if you do not poll.

An invalid property assignment keeps the previous value and enqueues `Error`. Methods that return `Result` may also enqueue an error.

- Events stay readable after `close()`. A repeated pause or close does not add another event.
- The queue holds 1,024 events. Overflow drops the oldest and adds `EventsLost` on the next drain.
- Position events are limited to four per second while state is observed. Poll once per frame.

`Waiting` means playback ran out of streamed data. Byte and buffer sources never emit it.

A stopped native device or a closed browser audio context is reported as device loss. Permission changes and hardware hotplug are not reported on every platform.

## Choose an output

Each player has its own `sink_id`, and an empty ID uses the system default. Switching keeps position, volume, mute, and looping; a failed switch leaves the old output active.

```dream
import system.audio;

async fun select_output(audio: Audio): Result<Unit, AudioError> {
    let devices = Audio.devices().await?;
    if devices.length == 0 {
        return Result.Err(AudioError("ENODEVICE", "no output device is visible"));
    }
    return audio.set_sink_id(devices[0].id).await;
}
```

IDs are opaque. Enumerate devices again instead of reusing an old ID.

- Browsers may hide labels until permission is granted.
- `Audio.request_device().await` opens the browser output picker and must run from a user action in a secure context. On native it returns `ENOTSUP`.
- Unsupported output selection and denied permission return errors. Starting playback in a browser also needs a user gesture.

## Inspect PCM and FFT data

Offline analysis does not open an audio device or include playback or decoders.
The following produces a single-sided spectrum for a generated tone:

```dream
import system;
import system.audio;

fun main(): void {
    let samples = [0.0f; 2048];
    for (let i = 0; i < samples.length; i = i + 1) {
        samples[i] = (float)Math.sin(6.283185307179586d * 440.0d * (double)i / 48000.0d);
    }
    let analyzer = AudioAnalyzer();
    let result = analyzer.analyze(samples, 48000, 1);
    if result.is_err() {
        System.println(result.unwrap_err().message());
        return;
    }
    let analysis = result.unwrap();
    System.println("Peak: " + analysis.peak + ", RMS: " + analysis.rms);
    System.println("Bin 19 frequency: " + analysis.frequency_hz(19).unwrap());
}
```

Samples are interleaved: left, right, left, right for stereo. The analyzer averages channels to mono, uses the latest FFT-sized block, and pads a short input with silence.

- The sample rate must be positive, frames complete, and samples finite.
- The default is 2,048 samples and a Hann window.
- `set_fft_size` accepts powers of two from 32 through 32,768. `AudioWindow.Rectangular` turns tapering off.

For size `N`, there are `N / 2 + 1` bins at `bin * sample_rate / N` Hz.

- `real` and `imaginary` are unnormalized complex values after the window.
- `magnitudes` divides magnitude by the sum of window weights and doubles every bin except DC and Nyquist. A unit sine centered on a bin has amplitude 1.
- `magnitudes_db` is `20 * log10(amplitude)`, floored at -120 dBFS. Peak and RMS use the unweighted, padded waveform.

`AudioBuffer.from_pcm` copies the PCM you pass. `read_pcm(start_frame, frame_count)` returns a checked copy.

`AudioBuffer.decode(bytes).await` detects WAV, MP3, or WebM/Opus without opening a device. It copies the input, decodes off your thread, limits decoded PCM with `max_bytes` (default 256 MiB), and accepts a `CancellationToken` to stop waiting:

```dream
import system;
import system.audio;
import system.io;

async fun main(): void {
    let bytes = File.read_bytes("clip.wav").await;
    if bytes.is_err() {
        System.println("could not read clip.wav");
        return;
    }

    let decoded = AudioBuffer.decode(bytes.unwrap()).await;
    if decoded.is_err() {
        System.println(decoded.unwrap_err().code() + ": " + decoded.unwrap_err().message());
        return;
    }

    let buffer = decoded.unwrap();
    System.println("Decoded " + buffer.duration + " seconds");
}
```

- Invalid input returns `EFORMAT`. More than two channels, or audio over the limit, returns `ELIMIT`.
- An exhausted decoder returns `ENOMEM`, no decoder thread returns `EBUSY`, and a cancelled wait returns `ECANCELLED`.
- A cancelled decode finishes in the background and its result is discarded.

`audio.capture(frame_count)` taps this player after gain and mute, before it mixes with other players. It returns stereo PCM at the output sample rate.

- The first calls can be empty while the tap starts.
- At most 32,768 frames are accepted. `frame_position` counts rendered frames since capture began.
- `overrun` means frames were dropped, or the snapshot was not consistent.

## Deployment

An optimized native build and a browser build drop an unused `system.audio` import. A debug native build keeps the type anyway.

- PCM playback does not include decoders or FFT. Analysis-only code does not include playback.
- Playing encoded audio includes every supported codec, because the format is read from the bytes.
- A string source also includes file and HTTP loading.

The [Audio API reference](../../reference/api/system-audio.md) lists declarations.
