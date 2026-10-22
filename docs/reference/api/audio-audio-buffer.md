# AudioBuffer

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioBuffer`

Immutable decoded interleaved PCM, independent of any output device.

```dream
public class AudioBuffer
```

## `decode`

Detects and decodes WAV, MP3 or WebM/Opus bytes on a background thread (a decode worker
on the web), so timers, input and audio playback keep running while a large file decodes.
The input is copied when decoding starts; changing `bytes` afterwards has no effect.

`max_bytes` limits the decoded 32-bit float PCM and must be between 4 bytes and 1 GiB.
Errors: EINVAL for an invalid limit, ELIMIT when the audio exceeds the limit or has more
than two channels, EFORMAT for invalid or unsupported input, ENOMEM when decoding memory
runs out, EBUSY when no decoder thread is available, and ECANCELLED when `token` (or the
ambient cancellation token) is cancelled before decoding ends.

```dream
public static async fun decode(
    bytes: byte[],
    max_bytes: int = 268435456,
    token: Option<CancellationToken> = Option.None
): Result<AudioBuffer, AudioError>
```

```dream
import system.audio;
import system.io;

async fun load_clip(path: string): Result<AudioBuffer, AudioError> {
    let bytes = File.read_bytes(path).await;
    if bytes.is_err() {
        return Result.Err(AudioError("EIO", "could not read " + path));
    }
    return AudioBuffer.decode(bytes.unwrap()).await;
}
```

## `from_pcm`

Copies finite interleaved mono/stereo PCM with complete frames and a positive sample rate.

```dream
public static fun from_pcm(
    samples: float[],
    sample_rate: int,
    channels: int
): Result<AudioBuffer, AudioError>
```

## `sample_rate`

Samples per second per channel.

```dream
public get sample_rate(): int
```

## `channels`

One for mono or two for stereo.

```dream
public get channels(): int
```

## `frame_count`

Number of complete frames in this buffer.

```dream
public get frame_count(): int
```

## `duration`

Total duration in seconds.

```dream
public get duration(): double
```

## `read_pcm`

Copies a frame range; invalid or overflowing ranges return EINVAL.

```dream
public fun read_pcm(start_frame: int, frame_count: int): Result<float[], AudioError>
```
