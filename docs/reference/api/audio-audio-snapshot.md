# AudioSnapshot

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioSnapshot`

A copied view of a player's recent output PCM.

```dream
public class AudioSnapshot
```

## `samples`

Interleaved PCM after the player's volume and mute settings.

```dream
public samples: float[]
```

## `sample_rate`

Samples per second per channel.

```dream
public sample_rate: int
```

## `channels`

One for mono or two for stereo.

```dream
public channels: int
```

## `frame_position`

Rendered frame index since capture was enabled, at the beginning of this snapshot.

```dream
public frame_position: long
```

## `overrun`

Whether output frames were lost to queue overflow or a concurrent capture update.

```dream
public overrun: bool
```
