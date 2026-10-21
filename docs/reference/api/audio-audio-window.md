# AudioWindow

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioWindow`

The weighting applied before computing an audio spectrum.

```dream
public enum AudioWindow
```

## `Hann`

Tapers both ends to reduce leakage between frequency bins.

```dream
Hann
```

## `Rectangular`

Leaves the input unchanged; useful for integer-period test signals.

```dream
Rectangular
```
