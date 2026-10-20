# AudioSnapshot

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

A copied view of a player's recent output PCM.

```dream
public class AudioSnapshot
```

## Declaration

Interleaved PCM after the player's volume and mute settings.

```dream
public samples: float[]
```

## Declaration

Samples per second per channel.

```dream
public sample_rate: int
```

## Declaration

One for mono or two for stereo.

```dream
public channels: int
```

## Declaration

Rendered frame index since capture was enabled, at the beginning of this snapshot.

```dream
public frame_position: long
```

## Declaration

Whether output frames were lost to queue overflow or a concurrent capture update.

```dream
public overrun: bool
```

