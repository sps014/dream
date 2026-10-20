# AudioOptions

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

Resource limits for a player. Values are snapshotted by its constructor.

```dream
public class AudioOptions
```

## Declaration

Maximum decoded PCM bytes for byte sources, which decode completely; defaults to 256 MiB.

```dream
public max_decoded_bytes: int
```

## Declaration

Maximum encoded bytes a streamed file or URL queues ahead of its decoder; defaults to 8 MiB. An encoded element that cannot fit, such as an oversized WebM frame, fails with ELIMIT. Track length is not limited.

```dream
public max_compressed_bytes: int
```

## Declaration

Decoded audio a streamed source keeps queued for output, in seconds; defaults to one. Values are clamped between 1,024 frames and 30 seconds.

```dream
public buffer_seconds: double
```

## Declaration

Creates the default resource limits.

```dream
public constructor()
```

