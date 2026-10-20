# AudioBuffer

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

Immutable decoded interleaved PCM, independent of any output device.

```dream
public class AudioBuffer
```

## Declaration

Detects and decodes encoded bytes, limiting decoded PCM to max_bytes.

```dream
public static fun decode(
    bytes: byte[],
    max_bytes: int = 268435456
): Result<AudioBuffer, AudioError>
```

## Declaration

Copies finite interleaved mono/stereo PCM with complete frames and a positive sample rate.

```dream
public static fun from_pcm(
    samples: float[],
    sample_rate: int,
    channels: int
): Result<AudioBuffer, AudioError>
```

## Declaration

Samples per second per channel.

```dream
public get sample_rate(): int
```

## Declaration

One for mono or two for stereo.

```dream
public get channels(): int
```

## Declaration

Number of complete frames in this buffer.

```dream
public get frame_count(): int
```

## Declaration

Total duration in seconds.

```dream
public get duration(): double
```

## Declaration

Copies a frame range; invalid or overflowing ranges return EINVAL.

```dream
public fun read_pcm(start_frame: int, frame_count: int): Result<float[], AudioError>
```

