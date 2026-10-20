# Audio

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

Whether the source reached natural completion without looping.

```dream
public get ended(): bool
```

## Declaration

Whether a seek is pending.

```dream
public get seeking(): bool
```

## Declaration

How much source data has been prepared for playback.

```dream
public get ready_state(): AudioReadyState
```

## Declaration

Most recent operation failure, cleared when the source is reset.

```dream
public get error(): Option<AudioError>
```

## Declaration

Opaque selected output ID; empty selects the system default.

```dream
public get sink_id(): string
```

## Declaration

Enumerates output devices visible to the application.

```dream
public static async fun devices(): Result<AudioDevice[], AudioError>
```

## Declaration

Prompts for browser output permission; native applications use devices instead.

```dream
public static async fun request_device(): Result<AudioDevice, AudioError>
```

## Declaration

Selects this player's output asynchronously; failure preserves the previous output.

```dream
public async fun set_sink_id(id: string): Result<bool, AudioError>
```

## Declaration

Enables an output tap and copies up to 32768 recent rendered frames. Initial calls may return no samples while capture starts. Output is stereo at the device rate.

```dream
public fun capture(frame_count: int): Result<AudioSnapshot, AudioError>
```

