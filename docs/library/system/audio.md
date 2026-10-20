---
title: Audio playback and FFT
summary: Play audio, select outputs, and inspect PCM and frequency data.
api: reference/api/system-audio.md
---

# Audio playback and FFT

Import `system.audio` to play WAV, MP3, and WebM/Opus on native desktops and in
browsers. Format detection inspects the bytes; the file extension is not required
to match. WebM decoding selects the first Opus track and ignores video tracks.
The initial implementation supports mono and stereo.

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

`Audio()` creates an empty player. Assign `src` before playing. Constructors do no
file I/O or device initialization. `Audio(bytes)` copies encoded bytes;
`Audio(buffer)` uses an already decoded `AudioBuffer`. Call `load().await` to
prepare a source without playing it. `play().await` prepares it if necessary and
returns when output starts. Node and standalone WASI support buffer decoding and
FFT analysis; speaker output is unavailable there.

`pause()` preserves position. `play().await` resumes. Set `current_time` in seconds
to seek; streamed sources seek as described under URLs and limits. `duration` is absent until loading completes.
`volume` ranges from zero to one; `muted` preserves its value. `loop` repeats a
loaded source. `close()` is terminal and idempotent. Keeping a player alive keeps
its playback resources alive; dropping it cancels loading and releases output.

## URLs and limits

String sources also accept HTTP and HTTPS URLs. Requests use Dream's HTTP client;
browser requests follow CORS and permission rules. Non-success HTTP responses
produce `EHTTP`. Replacing `src`, calling `load` again, or closing cancels the prior
load; its completion cannot overwrite the replacement source.

File and HTTP sources stream: playback starts once the first 256 KiB (or the
whole source, if smaller) has arrived and a quarter second of audio has been
decoded, and memory use does not grow with track length. `load()`
reads only enough to learn the format, channel count, and (when the container
declares it) the duration. Streamed MP3 files, and WebM files without a declared
duration, keep `duration` absent; playback, looping, and `Ended` still work.

If data arrives more slowly than it plays, output pauses with silence, `Waiting`
is reported, and `Playing` follows once data resumes. Seeking forward reads and
discards audio up to the target. Seeking backward reopens the file or repeats the
HTTP request and decodes from the start, so a backward seek in a long remote track
takes as long as downloading the skipped part. HTTP range requests are not used.
A seek on a stream is asynchronous: `seeking` stays true until `Seeked` arrives,
and assigning `current_time` again before then reports `EBUSY`. A target beyond a
known duration reports `EINVAL`.

On the web, decoding runs on the program's own thread rather than a background
worker, so a busy program can cause underruns.

```dream
import system.audio;

let options = AudioOptions();
options.max_compressed_bytes = 16 * 1024 * 1024;
options.max_decoded_bytes = 64 * 1024 * 1024;
let audio = Audio("https://example.com/song.webm", options);
```

For streamed sources, `max_compressed_bytes` (default 8 MiB) bounds the encoded
data held while decoding, not the track length. A file whose headers or a single
WebM element cannot be decoded within it fails with `ELIMIT`. `buffer_seconds`
(default one second, at most 30) sets how much decoded audio is queued ahead of
the speaker. `max_decoded_bytes` (default 256 MiB) applies to byte sources, which
are decoded completely; a larger result returns `ELIMIT`. Limits are snapshotted
by the constructor.

## Receive events

`poll_events()` drains a typed queue. Events contain monotonic timestamps in
nanoseconds. `Play` reports an accepted start and `Playing` reports running output.
`Seeking` and `Seeked` surround an accepted seek. `Pause`, `Ended`, `VolumeChange`,
`SinkChange`, and `Error` report the corresponding transitions. State properties
remain queryable if events are not being polled.

Invalid property assignments preserve the previous value and enqueue `Error`.
Fallible methods return `Result`; they may also enqueue an operation error.
Events remain readable after close. Repeated pause/close calls do not duplicate
those transitions. The queue holds 1,024 events; overflow discards the oldest and
adds an `EventsLost` record on the next drain. Position events are limited to four
per second and are generated while state is observed. Poll once per UI frame.

`Waiting` reports a streaming underrun; byte and buffer sources never produce it.
Current device-loss detection observes a stopped native device or closed browser
context. Browser permission changes and all forms of hardware hotplug have not
been qualified on every platform.

## Choose an output

Each player has its own `sink_id`; an empty ID uses the system default. Players
using the same device share a mixer internally. Switching preserves the source
position, volume, mute, and looping. A failed switch leaves the old output active.

```dream
import system.audio;

async fun select_output(audio: Audio): Result<bool, AudioError> {
    let devices = Audio.devices().await?;
    if devices.length == 0 {
        return Result.Err(AudioError("ENODEVICE", "no output device is visible"));
    }
    return audio.set_sink_id(devices[0].id).await;
}
```

IDs are opaque and host-specific. Enumerate again rather than assuming an old ID
still identifies the same device. Browsers may hide labels until permission has
been granted. `Audio.request_device().await` opens the browser's output picker and
must originate from a suitable user action in a secure context. Native programs
use enumeration; `request_device` returns `ENOTSUP`. Unsupported browser output
selection and denied permission return errors. Starting audio also requires
browser user activation.

## Inspect PCM and FFT data

Offline analysis does not open an audio device or include playback or decoders.
The following produces a single-sided spectrum for a generated tone:

```dream
import system;
import system.audio;

fun main(): void {
    let samples = Array.alloc<float>(2048);
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

Samples are interleaved: stereo frames are left, right, left, right. The analyzer
averages channels to mono, uses the latest FFT-sized block, and pads short input
with silence. Sample rate must be positive, frames complete, and samples finite.
It defaults to 2,048 samples and a Hann window. `set_fft_size` accepts powers of two
from 32 through 32,768; `AudioWindow.Rectangular` disables tapering.

For FFT size `N`, results contain `N / 2 + 1` bins at `bin * sample_rate / N` Hz.
`real` and `imaginary` are unnormalized complex components after windowing.
`magnitudes` divides complex magnitude by the sum of window weights and doubles
all bins except DC and Nyquist. A unit, bin-centered sine therefore has amplitude
one. `magnitudes_db` uses `20 * log10(amplitude)`, floored at -120 dBFS. Peak and RMS
use the unweighted, padded waveform.

`AudioBuffer.from_pcm` copies supplied PCM. `AudioBuffer.decode` detects encoded
bytes without a device. `read_pcm(start_frame, frame_count)` returns a checked copy.

`audio.capture(frame_count)` lazily enables a bounded tap after this player's
gain and mute and before mixing with other players. It returns stereo PCM at the
output sample rate. The first calls can return an empty snapshot while the tap
starts. Capture accepts at most 32,768 frames; `frame_position` counts rendered
frames since capture began. `overrun` reports dropped frames or a concurrent
update that prevented a consistent snapshot. Browser capture uses a bounded pool
of transferable buffers and requires no shared memory.

## Deployment

In optimized native deployments and browser WASM, an unused `system.audio` import
adds no audio code, hosts, or assets. Native debug binaries still retain unused
type metadata; byte-identical trimming there is not implemented yet. PCM playback
excludes decoders and FFT; analysis-only code excludes output hosts and decoders.
Automatic encoded-audio decoding includes all supported codecs because the source
format is discovered at runtime. String-source playback includes file/HTTP routing.

The [Audio API reference](../../reference/api/system-audio.md) lists declarations.
