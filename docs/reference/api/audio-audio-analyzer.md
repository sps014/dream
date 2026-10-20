# AudioAnalyzer

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## Declaration

Computes waveform levels and FFT data without opening an output device.

```dream
public class AudioAnalyzer
```

## Declaration

Creates a 2048-sample analyzer with a Hann window.

```dream
public constructor()
```

## Declaration

Selects a power-of-two FFT size from 32 through 32768; failure preserves the size.

```dream
public fun set_fft_size(size: int): Result<bool, AudioError>
```

## Declaration

Selects Hann tapering or a rectangular window.

```dream
public fun set_window(window: AudioWindow): void
```

## Declaration

Analyzes the latest FFT-sized block of interleaved mono or stereo samples. Short input is padded at the end; input must be finite with complete frames.

```dream
public fun analyze(
    samples: float[],
    sample_rate: int,
    channels: int
): Result<AudioAnalysis, AudioError>
```

