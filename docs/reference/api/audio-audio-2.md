# Audio

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `ended`

Whether the source reached natural completion without looping.

```dream
public get ended(): bool
```

## `seeking`

Whether a seek is pending.

```dream
public get seeking(): bool
```

## `ready_state`

How much source data has been prepared for playback.

```dream
public get ready_state(): AudioReadyState
```

## `error`

Most recent operation failure, cleared when the source is reset.

```dream
public get error(): Option<AudioError>
```

## `sink_id`

Opaque selected output ID; empty selects the system default.

```dream
public get sink_id(): string
```

## `devices`

Enumerates output devices visible to the application.

```dream
public static async fun devices(): Result<AudioDevice[], AudioError>
```

## `request_device`

Prompts for browser output permission; native applications use devices instead.

```dream
public static async fun request_device(): Result<AudioDevice, AudioError>
```

## `set_sink_id`

Selects this player's output asynchronously; failure preserves the previous output.

```dream
public async fun set_sink_id(id: string): Result<Unit, AudioError>
```

## `capture`

Enables an output tap and copies up to 32768 recent rendered frames. Initial calls may return no samples while capture starts. Output is stereo at the device rate.

```dream
public fun capture(frame_count: int): Result<AudioSnapshot, AudioError>
```
