# AudioEvent

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

Ordered player notifications. Timestamps are monotonic nanoseconds.

```dream
public enum AudioEvent
```

## Declaration

Loading began.

```dream
LoadStart(timestamp: long)
```

## Declaration

Source metadata became available.

```dream
LoadedMetadata(timestamp: long)
```

## Declaration

Samples are available for playback.

```dream
CanPlay(timestamp: long)
```

## Declaration

Playback was requested and pause state cleared.

```dream
Play(timestamp: long)
```

## Declaration

Output started or recovered from buffering.

```dream
Playing(timestamp: long)
```

## Declaration

Playback paused without changing position.

```dream
Pause(timestamp: long)
```

## Declaration

A seek was accepted; position is the requested time in seconds.

```dream
Seeking(timestamp: long, position: double)
```

## Declaration

A seek completed; position is the actual time in seconds.

```dream
Seeked(timestamp: long, position: double)
```

## Declaration

Playback position changed; emitted at most four times per second.

```dream
TimeUpdate(timestamp: long, position: double)
```

## Declaration

Playback is waiting for source data.

```dream
Waiting(timestamp: long)
```

## Declaration

The source reached its natural end without looping.

```dream
Ended(timestamp: long)
```

## Declaration

Volume or mute changed.

```dream
VolumeChange(timestamp: long)
```

## Declaration

The known duration changed.

```dream
DurationChange(timestamp: long)
```

## Declaration

Previous source state was reset.

```dream
Emptied(timestamp: long)
```

## Declaration

Pending loading was cancelled.

```dream
Abort(timestamp: long)
```

## Declaration

An operation failed; terminal marks a failure that stopped playback.

```dream
Error(timestamp: long, error: AudioError, operation: string, terminal: bool)
```

## Declaration

The selected output changed.

```dream
SinkChange(timestamp: long, sink_id: string)
```

## Declaration

Output disappeared and playback paused.

```dream
DeviceLost(timestamp: long, error: AudioError)
```

## Declaration

Playback resources were released.

```dream
Closed(timestamp: long)
```

## Declaration

Queue overflow discarded count older events.

```dream
EventsLost(timestamp: long, count: int)
```

