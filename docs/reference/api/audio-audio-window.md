# AudioWindow

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

The weighting applied before computing an audio spectrum.

```dream
public enum AudioWindow
```

## Declaration

Tapers both ends to reduce leakage between frequency bins.

```dream
Hann
```

## Declaration

Leaves the input unchanged; useful for integer-period test signals.

```dream
Rectangular
```

