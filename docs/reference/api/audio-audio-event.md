# AudioEvent

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioEvent`

Ordered player notifications. Timestamps are monotonic nanoseconds.

```dream
public enum AudioEvent
```

## `LoadStart`

Loading began.

```dream
LoadStart(timestamp: long)
```

## `LoadedMetadata`

Source metadata became available.

```dream
LoadedMetadata(timestamp: long)
```

## `CanPlay`

Samples are available for playback.

```dream
CanPlay(timestamp: long)
```

## `Play`

Playback was requested and pause state cleared.

```dream
Play(timestamp: long)
```

## `Playing`

Output started or recovered from buffering.

```dream
Playing(timestamp: long)
```

## `Pause`

Playback paused without changing position.

```dream
Pause(timestamp: long)
```

## `Seeking`

A seek was accepted; position is the requested time in seconds.

```dream
Seeking(timestamp: long, position: double)
```

## `Seeked`

A seek completed; position is the actual time in seconds.

```dream
Seeked(timestamp: long, position: double)
```

## `TimeUpdate`

Playback position changed; emitted at most four times per second.

```dream
TimeUpdate(timestamp: long, position: double)
```

## `Waiting`

Playback is waiting for source data.

```dream
Waiting(timestamp: long)
```

## `Ended`

The source reached its natural end without looping.

```dream
Ended(timestamp: long)
```

## `VolumeChange`

Volume or mute changed.

```dream
VolumeChange(timestamp: long)
```

## `DurationChange`

The known duration changed.

```dream
DurationChange(timestamp: long)
```

## `Emptied`

Previous source state was reset.

```dream
Emptied(timestamp: long)
```

## `Abort`

Pending loading was cancelled.

```dream
Abort(timestamp: long)
```

## `Error`

An operation failed; terminal marks a failure that stopped playback.

```dream
Error(timestamp: long, error: AudioError, operation: string, terminal: bool)
```

## `SinkChange`

The selected output changed.

```dream
SinkChange(timestamp: long, sink_id: string)
```

## `DeviceLost`

Output disappeared and playback paused.

```dream
DeviceLost(timestamp: long, error: AudioError)
```

## `Closed`

Playback resources were released.

```dream
Closed(timestamp: long)
```

## `EventsLost`

Queue overflow discarded count older events.

```dream
EventsLost(timestamp: long, count: int)
```
