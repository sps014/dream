# AudioReadyState

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioReadyState`

How much of an audio source is available for playback.

```dream
public enum AudioReadyState
```

## `Nothing`

No source metadata has been loaded.

```dream
Nothing
```

## `Metadata`

Source metadata is available but no playable samples are buffered.

```dream
Metadata
```

## `CurrentData`

Samples at the current position are available.

```dream
CurrentData
```

## `FutureData`

Samples at and beyond the current position are available.

```dream
FutureData
```
