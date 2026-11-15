# Audio

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `Audio`

A web-style audio player. Constructors configure sources; load and play perform I/O.

```dream
public class Audio
```

## `constructor`

Creates an empty player. Assign src before calling play.

```dream
public constructor()
```

## `constructor`

Creates a streaming player for a path or HTTP URL without starting I/O.

```dream
public constructor(src: string)
```

## `constructor`

Creates a streaming player for a path or URL with snapshotted buffering limits.

```dream
public constructor(src: string, options: AudioOptions)
```

## `constructor`

Snapshots encoded bytes; detection and decoding begin when loading starts.

```dream
public constructor(bytes: byte[])
```

## `constructor`

Snapshots encoded bytes with a configurable decoded-data limit.

```dream
public constructor(bytes: byte[], options: AudioOptions)
```

## `constructor`

Creates a player from already decoded PCM without including encoded-format decoders.

```dream
public constructor(buffer: AudioBuffer)
```

## `load`

Cancels prior loading and prepares the current source; returns an I/O or decoding error.

```dream
public async fun load(): Result<Unit, AudioError>
```

## `play`

Loads if necessary and starts or resumes playback. Browser activation may be required.

```dream
public async fun play(): Result<Unit, AudioError>
```

## `pause`

Pauses without changing position; repeating pause emits no duplicate event.

```dream
public fun pause(): Result<Unit, AudioError>
```

## `close`

Cancels loading and releases output. Queued events remain available after closure.

```dream
public fun close(): void
```

## `poll_events`

Drains ordered typed notifications, including after close.

```dream
public fun poll_events(): AudioEvent[]
```

## `src`

Current string source; empty for PCM and encoded-byte constructors.

```dream
public get src(): string
```

## `src`

Replaces the source, cancelling prior playback and pending loads.

```dream
public set src(value: string)
```

## `current_time`

Current position in seconds.

```dream
public get current_time(): double
```

## `current_time`

Requests an absolute seek in seconds; invalid requests preserve the current position.

```dream
public set current_time(value: double)
```

## `duration`

Source duration in seconds, absent until metadata is loaded and for streams that do not declare one, such as streamed MP3.

```dream
public get duration(): Option<double>
```

## `volume`

Linear gain from zero to one; mute does not change this value.

```dream
public get volume(): float
```

## `volume`

Sets finite gain from zero to one, preserving the previous value on error.

```dream
public set volume(value: float)
```

## `muted`

Whether this player is muted.

```dream
public get muted(): bool
```

## `muted`

Toggles mute without changing the stored volume.

```dream
public set muted(value: bool)
```

## `loop`

Whether natural completion restarts playback.

```dream
public get loop(): bool
```

## `loop`

Enables repetition of a seekable source.

```dream
public set loop(value: bool)
```

## `paused`

Whether playback is paused, including before the first play call.

```dream
public get paused(): bool
```
