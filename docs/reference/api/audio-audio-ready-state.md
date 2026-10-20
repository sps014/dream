# AudioReadyState

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

How much of an audio source is available for playback.

```dream
public enum AudioReadyState
```

## Declaration

No source metadata has been loaded.

```dream
Nothing
```

## Declaration

Source metadata is available but no playable samples are buffered.

```dream
Metadata
```

## Declaration

Samples at the current position are available.

```dream
CurrentData
```

## Declaration

Samples at and beyond the current position are available.

```dream
FutureData
```

